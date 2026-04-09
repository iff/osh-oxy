{
  description = "osh-oxy: fuzzy shell history search";

  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:NixOS/nixpkgs";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      self,
      flake-utils,
      nixpkgs,
      rust-overlay,
    }:

    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [
          (import rust-overlay)
          (self: super: {
            rustToolchain = pkgs.symlinkJoin {
              name = "rust-toolchain";
              paths = [
                (super.rust-bin.stable.latest.minimal.override {
                  extensions = [
                    "clippy"
                    "rust-docs"
                  ];
                })
                (super.rust-bin.selectLatestNightlyWith (toolchain: toolchain.rustfmt))
              ];
            };
          })
        ];

        pkgs = import nixpkgs { inherit system overlays; };

        osh = pkgs.rustPlatform.buildRustPackage {
          pname = "osh-oxy";
          version = "0.0.2";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = [ pkgs.pkg-config ];

          meta = {
            description = "fuzzy shell history search";
            homepage = "https://github.com/iff/osh-oxy";
            license = pkgs.lib.licenses.mit;
            mainProgram = "osh-oxy";
          };
        };
      in
      {
        packages = {
          app = osh;
          default = osh;
        };

        apps.default = {
          type = "app";
          program = "${osh}/bin/osh-oxy";
        };

        devShells.default = pkgs.mkShell {
          packages =
            with pkgs;
            [
              rustToolchain
              pkg-config
              cargo-deny
              cargo-edit
              cargo-watch
              rust-analyzer
              zizmor
              pinact
              hyperfine
            ]
            ++ lib.optionals pkgs.stdenv.isLinux [ glibc.debug ];

          shellHook = ''
            echo "Rust stable: $(${pkgs.rustToolchain}/bin/rustc --version)"
          '';
        };
      }
    );
}
