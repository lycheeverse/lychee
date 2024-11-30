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
      pkgs = nixpkgs.legacyPackages.${system};
      toolchain = fenix.packages.${system}.fromToolchainFile {
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

        nativeBuildInputs = [
          pkgs.pkg-config
        ];
        buildInputs = [
          pkgs.openssl
        ];

        cargoLock.lockFile = ./Cargo.lock;
      };
    });
}
