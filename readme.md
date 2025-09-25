# osh-oxy

**Note:** This is a very simple (and still crude) tool designed primarily for my personal workflows. It's intentionally kept minimal and focused. It provides simple fuzzy history search using our own history format, based on our Python [one-shell-history](https://github.com/dkuettel/one-shell-history).

Currently it offers two commands to append and search:

- append to osh history file
- search all \*.osh history files with skim

Example [zsh integration](https://github.com/iff/fleet/blob/14d6e4159f2db62a0bc2ccb4bcec85f8a796585e/home/modules/shell/zsh/zshrcd/osh.zsh).

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

## setup

I organise my osh files per host:

```
.osh
├── active
│   ├── host.osh
│   ├── name.osh
│   └── xyz.osh
└── local.osh -> active/host.osh
```
