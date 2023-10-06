{
  rustc,
  rustfmt,
  mkShell,
}:
mkShell {
  name = "nix-editor";
  packages = [
    rustc
    rustfmt
  ];
}
