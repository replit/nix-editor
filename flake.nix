{
  description = "A replit.nix editor.";

  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";

  outputs = { self, nixpkgs }: let
    systems = [
      "aarch64-darwin"
      "aarch64-linux"
      "x86_64-darwin"
      "x86_64-linux"
    ];
    eachSystem = nixpkgs.lib.genAttrs systems;
    rev =
      if self ? rev
      then "0.0.0-${builtins.substring 0 7 self.rev}"
      else "0.0.0-dirty";
  in {
    packages = eachSystem (system: let
      pkgs = nixpkgs.legacyPackages.${system};
    in rec {
      default = nix-editor;
      nix-editor = pkgs.callPackage ./nix-editor.nix {
        inherit rev;
      };
    });
  };
}
