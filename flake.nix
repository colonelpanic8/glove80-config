{
  description = "Ivan's Glove80 ZMK configuration with per-layer RGB maps";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-22.05";
    flake-utils.url = "github:numtide/flake-utils";
    zmk = {
      url = "github:darknao/zmk/darknao/rgb-dts";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, flake-utils, zmk }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        firmware = import zmk { inherit pkgs; };
      in
      {
        packages.firmware = import ./config {
          inherit pkgs firmware;
        };
        packages.default = self.packages.${system}.firmware;

        apps.generate-keymap = {
          type = "app";
          program = "${pkgs.writeShellScript "generate-glove80-keymap" ''
            exec ${pkgs.nodejs}/bin/node "$PWD/scripts/generate-keymap.mjs"
          ''}";
        };

        devShells.default = pkgs.mkShell {
          packages = [
            pkgs.nodejs
            pkgs.nixpkgs-fmt
          ];
        };
      });
}
