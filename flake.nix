{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    inputs@{ self, flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      perSystem =
        {
          config,
          self',
          inputs',
          pkgs,
          system,
          ...
        }:
        {
          _module.args.pkgs = import self.inputs.nixpkgs {
            inherit system;
            overlays = [
              (import inputs.rust-overlay)
            ];
          };
          devShells.default =
            let
              rustVersion = "latest"; # using a specific version: "1.62.0"
              rust = pkgs.rust-bin.stable.${rustVersion}.default.override {
                extensions = [
                  "rust-src" # for rust-analyzer
                  "rust-analyzer" # usable by IDEs like zed-editor
                  "clippy"
                ];
              };
              libPath =
                with pkgs;
                lib.makeLibraryPath [
                  pkg-config
                  openssl
                ];

            in
            pkgs.mkShell {
              inputsFrom = builtins.attrValues self'.packages;
              packages = [
                pkgs.pkg-config
                pkgs.openssl
                rust
              ];
              LD_LIBRARY_PATH = libPath;
              RUST_BACKTRACE = 1;
              LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
            };
        };
    };
}
