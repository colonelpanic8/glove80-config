#!/usr/bin/env bash
# Build both Glove80 RMK halves and produce flashable UF2 images.
#
# bindgen (nrf-mpsl-sys / nrf-sdc-sys) needs libclang; on NixOS we take it
# from nixpkgs and point clang at its own freestanding headers so host glibc
# headers never leak into the arm-none-eabi bindings.
set -euo pipefail
cd "$(dirname "$0")"

LIBCLANG=$(nix build --no-link --print-out-paths nixpkgs#libclang.lib)
RESOURCE_INCLUDE=$(ls -d "$LIBCLANG"/lib/clang/*/include | head -1)
export LIBCLANG_PATH="$LIBCLANG/lib"
export BINDGEN_EXTRA_CLANG_ARGS="-ffreestanding -nostdinc -isystem $RESOURCE_INCLUDE"

cargo build --release --bin glove80_lh
cargo build --release --bin glove80_rh

node ../../scripts/elf-to-uf2.mjs \
    --elf target/thumbv7em-none-eabihf/release/glove80_lh \
    --family 0x9807B007 --out glove80_lh_rmk.uf2
node ../../scripts/elf-to-uf2.mjs \
    --elf target/thumbv7em-none-eabihf/release/glove80_rh \
    --family 0x9808B007 --out glove80_rh_rmk.uf2
