{
  description = "osh-oxy: fzf shell history search";

  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:NixOS/nixpkgs";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    { self
    , flake-utils
    , nixpkgs
    , rust-overlay
    }:

    flake-utils.lib.eachDefaultSystem (system:
    let
      overlays = [
        (import rust-overlay)
        (self: super: {
          rustToolchain =
            let
              rust = super.rust-bin;
            in
            if builtins.pathExists ./rust-toolchain.toml then
              rust.fromRustupToolchainFile ./rust-toolchain.toml
            else if builtins.pathExists ./rust-toolchain then
              rust.fromRustupToolchainFile ./rust-toolchain
            else
              rust.stable.latest.default;

          rustNightly = super.rust-bin.selectLatestNightlyWith (toolchain: toolchain.default.override {
            extensions = [ "rustfmt" ];
          });
        })
      ];

      pkgs = import nixpkgs { inherit system overlays; };

      rustfmt-nightly = pkgs.writeShellScriptBin "rustfmt" ''
        exec ${pkgs.rustNightly}/bin/rustfmt "$@"
      '';

      app = pkgs.rustPlatform.buildRustPackage {
        pname = "osh-oxy";
        version = "0.0.1";
        src = ./.;

        cargoLock = {
          lockFile = ./Cargo.lock;
        };

        nativeBuildInputs = [ pkgs.pkg-config ];
      };

      bins = [ rustfmt-nightly ];

    in
    {
      packages = {
        app = app;
        default = app;
      };

      apps.default = {
        type = "app";
        program = "${app}/bin/osh-oxy";
      };

      devShells.default = pkgs.mkShell {
        nativeBuildInputs = with pkgs; [
          rustToolchain
          pkg-config
          cargo-deny
          cargo-edit
          cargo-watch
          rust-analyzer
        ];

        shellHook = ''
          export PATH=${pkgs.lib.makeBinPath bins}:$PATH
          echo "Rust stable: $(${pkgs.rustToolchain}/bin/rustc --version)"
          echo "Nightly rustfmt: $(rustfmt --version)"
        '';
      };
    });
}
