# Glove80 configuration

Ivan's source-controlled Glove80 keymap, applied to the current
[`glove80-rmk`](https://github.com/colonelpanic8/glove80-rmk) firmware through
RMK's native Rynk protocol.

This repository deliberately contains configuration only. Firmware, hardware
support, lighting, the control CLI, release packaging, and their documentation
live in the pinned `dependencies/glove80-rmk` submodule. The former vendored ZMK
tree and the pre-extraction RMK product stack have been removed.

## Setup

```sh
make init
make check
```

`make init` initializes `glove80-rmk` and its pinned RMK submodule. `make check`
builds the pinned `glove80-control` and validates `config/glove80.toml` offline.
Nix supplies the Rust and native dependencies used by the control tool.

## Apply the keymap

Connect the keyboard over USB or BLE, then run:

```sh
make apply
make show
```

The keymap is written through Rynk, becomes active immediately, and is persisted
by RMK. Writes are verified in pages but are not atomic across the entire
keymap; an interrupted apply can leave earlier pages updated.

For transport selection or any other CLI command, use the pinned wrapper:

```sh
./bin/glove80-control --usb keymap read --all
./bin/glove80-control --ble version
./bin/glove80-control devices
```

Run `./bin/glove80-control --help` for the complete interface.

## Firmware

Build release firmware from the exact pinned product stack with:

```sh
make firmware
```

Artifacts are written under `dependencies/glove80-rmk/dist/`. The firmware's
compiled defaults currently match this keymap, while this repository remains
the editable source of truth for subsequent runtime changes.

## Updating `glove80-rmk`

Update deliberately, inspect the upstream changes, and then commit the new
gitlink:

```sh
git submodule update --remote dependencies/glove80-rmk
git -C dependencies/glove80-rmk log --oneline --decorate ORIG_HEAD..HEAD
make check
git add dependencies/glove80-rmk
```

The submodule tracks upstream `master`, but ordinary clones and builds always
use the exact commit recorded by this repository.
