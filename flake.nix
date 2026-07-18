{
  description = "Ivan's Glove80 ZMK Studio configuration";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-22.05";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        zmk = ./zmk;
        pkgs = import nixpkgs { inherit system; };
        zmkPkgs = import (zmk + "/nix/pinned-nixpkgs.nix") { inherit system; };
        firmware = import zmk { pkgs = zmkPkgs; };
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
