# Glove80 configuration

Ivan's source-controlled Glove80 keymap, applied to the current
[`glove80-rmk`](https://github.com/colonelpanic8/glove80-rmk) firmware through
RMK's native Rynk protocol.

Personal keymap and lighting policy live in this repository. Firmware hardware
support, reusable lighting/protocol machinery, the control CLI, and release
packaging live in the pinned `dependencies/glove80-rmk` submodule. The firmware
build injects [`config/firmware.toml`](config/firmware.toml) through RMK's
external keyboard-configuration path; `glove80-rmk` contains no personal
lighting rules.

## Setup

```sh
just init
just check
```

`just init` initializes `glove80-rmk` and its pinned RMK submodule. `just check`
builds the pinned `glove80-control` and validates `config/glove80.toml` offline.
Nix supplies the Rust and native dependencies used by the control tool.

## Apply the keymap

Connect the keyboard over USB or BLE, then run:

```sh
just apply
just show
```

The keymap is written through Rynk, becomes active immediately, and is persisted
by RMK. Writes are verified in pages but are not atomic across the entire
keymap; an interrupted apply can leave earlier pages updated.

For transport selection or any other CLI command, use the pinned wrapper:

```sh
./bin/glove80-control --usb keymap read --all
./bin/glove80-control --ble version
./bin/glove80-control --usb lighting caps
```

Run `./bin/glove80-control --help` for the complete interface.

## Firmware

Build release firmware from the exact pinned product stack with:

```sh
just firmware
```

Artifacts are written under `dependencies/glove80-rmk/dist/`. The firmware's
compiled defaults currently match this keymap, while this repository remains
the editable source of truth for subsequent runtime changes.

The build embeds three independently checkable Git identities in the Rynk
firmware label: this configuration repository's commit, the pinned
`glove80-rmk` commit and semver, and the pinned RMK submodule's full
`git describe` identity. Rynk also reports RMK's structured semantic version.
The release manifest records the full configuration, product, and RMK commits.
A dirty working tree is marked in both places.

## Lighting controls and indicators

Lighting has a three-state output policy: always on, always off, or on only
while USB power is present.

- Hold the left-thumb Magic key to temporarily wake lighting and show the
  information view without changing the selected policy.
- Press `Magic+F` to cycle always on → always off → plugged-in only. `F` is
  white as the action legend; adjacent `G` reports the selected policy in
  green, red, or blue respectively.
- While lighting is on, `F1` through `F5` show layers 0 through 4: green means
  active and dim red means inactive.
- While Games (layer 3) is active, `W`, `A`, `S`, and `D` are red. The
  left-thumb Backspace position is amber because its Games action is Space.
- While Magic is held, the five keys below the top key in each outer column
  form bottom-up battery bars for the corresponding half. Each segment is a
  20% band; green is normal, amber/red is low, and blue means charging.

The top-left outer-column key is reserved for the `F1` layer indicator, so the
battery bars intentionally use five segments rather than six.

## Updating `glove80-rmk`

Update deliberately, inspect the upstream changes, and then commit the new
gitlink:

```sh
git submodule update --remote dependencies/glove80-rmk
git -C dependencies/glove80-rmk log --oneline --decorate ORIG_HEAD..HEAD
just check
git add dependencies/glove80-rmk
```

The submodule tracks upstream `master`, but ordinary clones and builds always
use the exact commit recorded by this repository.
