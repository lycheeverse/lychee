{
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
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ (import rust-overlay) ];
      };
    in
    {
      devShells.${system}.default =
        let
          rustVersion = "latest"; # using a specific version: "1.62.0"
          rust = pkgs.rust-bin.stable.${rustVersion}.default.override {
            extensions = [
              "rust-src" # for rust-analyzer
              "rust-analyzer" # usable by IDEs like zed-editor
              "clippy"
            ];
          };
          libPath = pkgs.lib.makeLibraryPath [
            pkgs.pkg-config
            pkgs.openssl
          ];
        in
        pkgs.mkShell {
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
}
