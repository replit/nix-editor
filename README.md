***NOTE***: this package is deprecated. Use [UPM](https://github.com/replit/upm) with the `replit-nix` language to add deps to your Replit project instead!

There's no plans to support other features in UPM.

---

This is the replit.nix editor that is used by Goval to modify programatically interact with the file.

It parses the file into an AST and traverses the AST to get the relevant information to modify the file.

run `cargo run -- --help` to see what cli arguments are available.

You can directly add/remove packages through the cli args like so `cargo run -- --add pkgs.cowsay` or `cargo run -- --remove pkgs.cowsay` or `cargo run -- --get`.

You can also run it without passing in any flags. If you do that, it reads json from stdin with the following structure:
```
{"op":"add", "dep": "pkgs.cowsay" }
```


