{
  description = "Many small, independent builds for exercising a Nix CI scheduler";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs =
    { self, nixpkgs }:
    let
      pkgs = nixpkgs.legacyPackages.x86_64-linux;
      settings = builtins.fromJSON (builtins.readFile ./load.json);
      # Every commit gets new outputs: rerunning a commit reuses its results,
      # a new commit builds all of them again.
      salt = self.rev or self.dirtyRev or "uncommitted";
      # Keeps one core busy, so the build is visibly in progress on its machine.
      check =
        name:
        pkgs.runCommand "load-${name}" { inherit salt; } ''
          end=$((SECONDS + ${toString settings.seconds}))
          while [ "$SECONDS" -lt "$end" ]; do :; done
          echo ${name} > "$out"
        '';
      names = builtins.genList (i: pkgs.lib.fixedWidthNumber 4 (i + 1)) settings.checks;
    in
    {
      checks.x86_64-linux = builtins.listToAttrs (
        map (name: {
          name = "load-${name}";
          value = check name;
        }) names
      );
    };
}
