# osh-oxy

- simple fuzzy shell history search using our own format
- a Rust reimplementation of our [one-shell-history](https://github.com/dkuettel/one-shell-history)
- append to osh history file
- search all \*.osh history files with skim

## installation (nix flake)

build with nix:

```
nix build github:iff/osh-oxy
```

using osh-oxy with flakes:

```
osh-oxy = {
  url = "github:iff/osh-oxy";
  inputs.nixpkgs.follows = "nixpkgs";
  flake = true;
}
```
