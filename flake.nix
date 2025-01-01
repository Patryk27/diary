{
  inputs = {
    crane = {
      url = "github:ipetkov/crane";
    };

    nixpkgs = {
      url = "github:nixos/nixpkgs/nixos-unstable";
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

  outputs =
    {
      self,
      crane,
      nixpkgs,
      rust-overlay,
      utils,
    }:
    utils.lib.eachDefaultSystem (
      system:
      let
        inherit (pkgs) lib;

        pkgs = import nixpkgs {
          inherit system;

          overlays = [
            (import rust-overlay)
          ];
        };

        crane' = (crane.mkLib pkgs).overrideToolchain (
          pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain
        );

        deps = with pkgs; [
          exiftool
        ];

        app = crane'.buildPackage {
          src = ./.;
          doCheck = true;

          buildInputs =
            with pkgs;
            [
              makeWrapper
            ]
            ++ deps;

          postInstall = ''
            wrapProgram $out/bin/diary \
              --set PATH ${lib.makeBinPath deps}
          '';
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
