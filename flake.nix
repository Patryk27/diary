{
  inputs = {
    crane = {
      url = "github:ipetkov/crane";

      inputs = {
        nixpkgs = {
          follows = "nixpkgs";
        };
      };
    };

    nixpkgs = {
      url = "github:nixos/nixpkgs";
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";

      inputs = {
        nixpkgs = {
          follows = "nixpkgs";
        };
      };
    };

    utils = {
      url = "github:numtide/flake-utils";
    };
  };

  outputs = { self, crane, nixpkgs, rust-overlay, utils }:
    utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;

          overlays = [
            (import rust-overlay)
          ];
        };

        crane' =
          (crane.mkLib pkgs).overrideToolchain
            (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain);

        app = crane'.buildPackage {
          src = ./.;
          doCheck = true;

          propagatedBuildInputs = with pkgs; [
            exiftool
            ffmpeg-full
          ];
        };

      in
      {
        packages = {
          default = app;
        };

        devShells = {
          default = crane'.devShell {
            inputsFrom = [ app ];
          };
        };
      }
    );
}
