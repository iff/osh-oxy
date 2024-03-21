# osh-oxy

- simple shell history fzf search using our own format
- a Rust reimplementation of https://github.com/dkuettel/one-shell-history

what works:

- append to osh history file - format will change soon
- search all \*.osh history files with fzf

what is not supported yet:

- filter by session id and other aggregation

## dependencies

- fzf

## installation (nix flake)

`nix build github:iff/osh-oxy`

if you use this in your flakes:

```
osh-oxy = {
  url = "github:iff/osh-oxy";
  inputs.nixpkgs.follows = "nixpkgs";
  flake = true;
}
```

## todo

- robust output parsing
- fix preview
- support different ways of merging/sorting history search (session, ...)
- use rust native fzf library?
