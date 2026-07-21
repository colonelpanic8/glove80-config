{
  description = "Glove80 configuration and RMK agent-attention daemon";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

  outputs = {self, nixpkgs}: let
    systems = ["x86_64-linux" "aarch64-linux"];
    forAllSystems = function:
      nixpkgs.lib.genAttrs systems (system: function (import nixpkgs {inherit system;}));
  in {
    packages = forAllSystems (pkgs: let
      source = pkgs.lib.fileset.toSource {
        root = ./.;
        fileset = pkgs.lib.fileset.unions [
          ./Cargo.toml
          ./Cargo.lock
          ./src
        ];
      };
      package = pkgs.rustPlatform.buildRustPackage {
        pname = "rmk-agent-attention";
        version = "0.1.0";
        src = source;
        cargoLock.lockFile = ./Cargo.lock;
      };
    in {
      rmk-agent-attention = package;
      default = package;
    });

    apps = forAllSystems (pkgs: {
      default = {
        type = "app";
        program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.default}/bin/rmk-attentiond";
        meta.description = "Drive RMK keyboard lighting from coding-agent attention events";
      };
    });

    devShells = forAllSystems (pkgs: {
      default = pkgs.mkShell {
        packages = [pkgs.cargo pkgs.rustc pkgs.rustfmt];
      };
    });
  };
}
