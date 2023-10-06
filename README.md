This program edits Replit's replit.nix editor that is used by Goval to modify programatically interact with the file.

It parses the file into an AST and traverses the AST to get the relevant information to modify the file.

run `cargo run -- --help` to see what cli arguments are available.

```
nix-editor

USAGE:
    nix-editor [OPTIONS]

OPTIONS:
    -a, --add <ADD>              
    -d, --dep-type <DEP_TYPE>    [default: regular] [possible values: regular, python]
    -h, --human                  
        --help                   Print help information
    -p, --path <PATH>            
    -r, --remove <REMOVE>        
        --return-output          
    -v, --verbose                
    -V, --version                Print version information
```

You can directly add/remove packages through the cli args like so `cargo run -- --add pkgs.cowsay` or `cargo run -- --remove pkgs.cowsay` or `cargo run -- --get`.

You can also run it without passing in any flags. If you do that, it reads json from stdin with the following structure:
```
{"op":"add", "dep": "pkgs.cowsay" }
```

# Contributing

* Please run `nix fmt` to format the code in this repository before making a pull request.
* `nix develop` will put you in a devshell with all the necessary development tools.
