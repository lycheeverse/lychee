{
  description = "A flake for building and developing lychee";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      ...
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      rustVersion = "latest"; # using a specific version: "1.62.0"
    in
    {
      devShells = forAllSystems (system: {
        default =
          let
            pkgs = import nixpkgs {
              inherit system;
              overlays = [ (import rust-overlay) ];
            };
            rust = pkgs.rust-bin.stable.${rustVersion}.default.override {
              extensions = [
                "rust-src" # for rust-analyzer
                "rust-analyzer" # usable by IDEs like zed-editor
                "clippy"
              ];
            };
          in
          pkgs.mkShell {
            packages = [
              pkgs.pkg-config
              pkgs.openssl
              rust
            ];
          };
      });

      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          code = pkgs.callPackage ./. {
            inherit
              nixpkgs
              system
              rust-overlay
              rustVersion
              ;
          };
        in
        {
          default = pkgs.symlinkJoin {
            name = "all";
            paths = with code; [ app ];
          };
        }
      );
    };
}