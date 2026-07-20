{
  description = "Glove80 RMK firmware and host tooling";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, fenix, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        zmk = ./zmk;
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;
        rustToolchain = fenix.packages.${system}.fromToolchainFile {
          file = ./rust-toolchain.toml;
          sha256 = "sha256-OATSZm98Es5kIFuqaba+UvkQtFsVgJEBMmS+t6od5/U=";
        };
        libclang = pkgs.llvmPackages.libclang.lib;
        clangMajor = lib.versions.major pkgs.llvmPackages.clang.version;
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
            rustToolchain
            pkgs.nodejs
            pkgs.nixpkgs-fmt
            pkgs.pkg-config
            pkgs.dbus.dev
            libclang
          ];

          LIBCLANG_PATH = "${libclang}/lib";
          BINDGEN_EXTRA_CLANG_ARGS =
            "-ffreestanding -nostdinc -isystem ${libclang}/lib/clang/${clangMajor}/include";
          CARGO_NET_GIT_FETCH_WITH_CLI = "true";
          # Fenix distributes rustup-style binaries with a conventional
          # Linux interpreter. Point nix-ld at this shell's glibc rather than
          # inheriting the host generation, which may be older than libclang.
          NIX_LD = pkgs.stdenv.cc.bintools.dynamicLinker;
          NIX_LD_LIBRARY_PATH = lib.makeLibraryPath [
            pkgs.glibc
            pkgs.stdenv.cc.cc.lib
          ];
          # rynk-ble uses BlueZ through bluer's libdbus backend. Cargo can
          # link it via pkg-config; keep the resulting CLI runnable from the
          # development shell as well.
          LD_LIBRARY_PATH = lib.makeLibraryPath [ pkgs.dbus ];
        };
      });
}
