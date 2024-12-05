{
  description = "Lychee - A fast, async, stream-based link checker";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, crane, ... }:
    let testingFixtures = [ ./fixtures ./README.md ];
    in {
      packages = nixpkgs.lib.mapAttrs (_: pkgs: {
        default = (crane.mkLib pkgs).buildPackage {
          pname = "lychee";
          version = self.rev or self.dirtyShortRev;

          src = with nixpkgs.lib.fileset;
            toSource {
              root = ./.;
              fileset = intersection (gitTracked ./.) (unions ([
                ./Cargo.lock
                (fileFilter (file: file.hasExt "rs" || file.hasExt "toml") ./.)
              ] ++ testingFixtures));
            };

          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.openssl ];
        };
      }) nixpkgs.legacyPackages;
    };
}
