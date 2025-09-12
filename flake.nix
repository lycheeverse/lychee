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
      self,
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
      devShells = forAllSystems (
        system:
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
        {
          default = pkgs.mkShell {
            packages = [
              pkgs.pkg-config
              pkgs.openssl
              rust
              pkgs.cargo-nextest
              pkgs.cargo-msrv
            ];
          };
        }
      );

      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.lychee.overrideAttrs {
            src = ./.;
            version = self.rev or self.dirtyShortRev;
            cargoDeps = pkgs.rustPlatform.importCargoLock {
              lockFile = ./Cargo.lock;
            };
          };
        }
      );
    };
}
