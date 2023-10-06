{
  rustPlatform,
  rev,
}:
rustPlatform.buildRustPackage {
  pname = "nix-editor";
  version = rev;

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  src = ./.;
}
