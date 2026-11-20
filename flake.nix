{
  description = "Many small builds for exercising a Nix CI scheduler";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs =
    { self, nixpkgs }:
    let
      pkgs = nixpkgs.legacyPackages.x86_64-linux;
      given = builtins.fromJSON (builtins.readFile ./load.json);
      settings = {
        # The first `fresh` checks are new in every commit. The rest are the
        # same in every commit that shares `generation`, so a scheduler builds
        # them once and skips them in later suites.
        fresh = given.checks;
        generation = "";
        # Of the fresh checks: the first `failing` fail, and they are stacked
        # in `tiers` layers, each check depending on the one below it.
        failing = 0;
        tiers = 1;
      }
      // given;
      commit = self.rev or self.dirtyRev or "uncommitted";
      # One duration, or a list the checks cycle through.
      durations = pkgs.lib.toList settings.seconds;
      layer = (settings.fresh + settings.tiers - 1) / settings.tiers;
      name = i: "load-${pkgs.lib.fixedWidthNumber 4 i}";
      # Keeps one core busy, so the build is visibly in progress on its machine.
      check =
        i:
        let
          seconds = builtins.elemAt durations (pkgs.lib.mod (i - 1) (builtins.length durations));
        in
        pkgs.runCommand (name i)
          (
            {
              salt = if i <= settings.fresh then commit else settings.generation;
            }
            // pkgs.lib.optionalAttrs (i <= settings.fresh && i > layer) {
              below = builtins.elemAt checks (i - layer - 1);
            }
          )
          ''
            end=$((SECONDS + ${toString seconds}))
            while [ "$SECONDS" -lt "$end" ]; do :; done
            ${if i <= settings.failing then "exit 1" else ''echo ${name i} > "$out"''}
          '';
      checks = builtins.genList (i: check (i + 1)) settings.checks;
    in
    {
      checks.x86_64-linux = builtins.listToAttrs (
        map (value: {
          inherit (value) name;
          inherit value;
        }) checks
      );
    };
}
