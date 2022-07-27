{ stdenv, git, runCommand, copyPathToStore, rev, lib, defaultCrateOverrides, buildRustCrate, buildPackages, fetchurl }@pkgs:
let
  generatedBuild = import ./Cargo.nix { inherit pkgs; };
  crate2nix = generatedBuild.workspaceMembers.nix-editor.build;
in stdenv.mkDerivation {
    pname = "nix-editor";
    version = rev;

    src = crate2nix;

    installPhase = ''
      cp -r ${crate2nix} $out
    '';
}

