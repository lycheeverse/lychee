{ pkgs }:
let
  version =
    let
      cargoToml = builtins.readFile ./Cargo.toml;
      match = builtins.match ''.*version = "([^"]+)".*'' cargoToml;
    in
    if match != null then builtins.elemAt match 0 else throw "Version not found in Cargo.toml";
in
{
  app = pkgs.rustPlatform.buildRustPackage {
    pname = "lychee";
    inherit version;
    src = ./.;

    cargoLock = {
      lockFile = ./Cargo.lock;
    };

    nativeBuildInputs = [ pkgs.pkg-config ];
    buildInputs =
      [ pkgs.openssl ]
      ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
        pkgs.Security
        pkgs.SystemConfiguration
      ];

    PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
    RUST_BACKTRACE = 1;

    checkFlags = [
      "--skip=src/lib.rs"
      "--skip=client::tests"
      "--skip=collector::tests::test_url_without_extension_is_html"
    ];
  };
}
