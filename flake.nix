{
  description = "Lychee - A fast, async, stream-based link checker";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix.url = "github:nix-community/fenix";
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    fenix,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {inherit system;};
      toolchain = fenix.packages.aarch64-darwin.fromToolchainFile {
        file = ./rust-toolchain.toml;
        sha256 = "sha256-yMuSb5eQPO/bHv+Bcf/US8LVMbf/G/0MSfiPwBhiPpk=";
      };
      platform = pkgs.makeRustPlatform {
        cargo = toolchain;
        rustc = toolchain;
      };
    in {
      packages.default = platform.buildRustPackage {
        pname = "lychee";
        version = self.rev or self.dirtyShortRev;

        src = pkgs.lib.cleanSource ./.;

        PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
        nativeBuildInputs = [
          pkgs.openssl
          pkgs.pkg-config
        ];

        cargoLock.lockFile = ./Cargo.lock;
      };
    });
}
