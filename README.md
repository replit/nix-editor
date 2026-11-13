# NixCI staging synthetic load

This branch holds only a flake that generates synthetic build load for a NixCI
scheduler. Its `checks.x86_64-linux` are many independent builds; each keeps one
CPU core busy for a fixed time and then writes its name.

`load.json` sets how many builds there are and how long each one runs, in
seconds:

```json
{ "checks": 1000, "seconds": 5400 }
```

Every build depends on the commit it was evaluated from. Building a commit a
second time reuses its results; any new commit, even an empty one, makes every
build new again:

```sh
git commit --allow-empty -m "Run the load again"
```

Evaluate it locally with `nix flake show` or build a single check with
`nix build .#checks.x86_64-linux.load-0001`.
