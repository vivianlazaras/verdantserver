{
  description = "Rust crate build with native OpenSSL linking";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };

        # Use the Rust toolchain from nixpkgs (you can pin or override if desired)
        rustToolchain = pkgs.rustPackages.stable;
      in {
        packages.default = rustToolchain.buildPackage {
          pname = "my-crate";
          version = "0.1.0";
          src = ./.;

          # Add OpenSSL, pkg-config, and other native deps
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.openssl ];

          # Ensure Cargo can find OpenSSL properly
          RUSTFLAGS = "-C link-args=-L${pkgs.openssl.out}/lib";
          OPENSSL_NO_VENDOR = 1;
          OPENSSL_DIR = "${pkgs.openssl.out}";

          # Optional: enables tests
          doCheck = true;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain.rustc
            rustToolchain.cargo
            pkgs.openssl
            pkgs.pkg-config
          ];

          OPENSSL_NO_VENDOR = 1;
          OPENSSL_DIR = "${pkgs.openssl.out}";
        };
      });
