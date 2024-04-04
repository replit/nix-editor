{
  writeShellApplication,
  alejandra,
  rustfmt,
}:
writeShellApplication {
  name = "fmt";
  text = ''
    echo "Formatting Nix code..."
    ${alejandra}/bin/alejandra -q .
    echo "Formatting Rust code..."
    ${rustfmt}/bin/cargo-fmt
  '';
}
