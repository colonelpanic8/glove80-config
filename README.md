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
just diff
just apply
```

`config/glove80.toml` is a bidirectional representation of managed runtime
state: keymap layers, default layer, brightness/background, output mode,
durable layer scenes and policy, and the generic lighting-extension selection.
Rynk supplies extension effect and palette names without knowing which effect
pack implements them.

`just diff` compares that TOML with the connected keyboard. `just apply` writes
only differences and verifies the resulting state. Keymap/default-layer and
durable-scene storage survive reboot; other lighting values are live state whose
boot defaults come from firmware. Key writes are not atomic across the entire
keymap, while durable lighting scenes are replaced atomically.

To inspect or pull state in the other direction:

```sh
just show                         # print canonical live TOML
just pull                         # rewrite config/glove80.toml from the keyboard
./bin/glove80-control config pull /tmp/glove80.toml
```

Pull preserves existing layer IDs and names when it can parse the destination,
but rewrites the file in canonical TOML form and therefore does not preserve
comments.

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
while USB power is present. In plugged-in-only mode each half evaluates its
own VBUS independently; USB power does not need to be the selected transport.
The final hardware driver caps each color channel at 230/255 (about 90%).

- Hold the left-thumb Magic key to temporarily wake lighting and show the
  information view without changing the selected policy.
- Press `Magic+A` to cycle always on → always off → plugged-in only. `A`
  reports the selected policy in green, red, or blue respectively.
- On Magic, `S`/`D` lower and raise overall brightness, while `X`/`C`/`V`
  toggle PaletteFX, advance to the next effect, and advance to the next
  palette. The one-shot adjustment/cycle controls are white; `X` retains its
  green toggle color, while `A` reports output mode in green, red, or blue.
  PaletteFX starts off and toggles on at half brightness.
- While lighting is on, number keys `1` through `4` show non-default layers 1
  through 4 in blue while active. Inactive layers are transparent/dark; layer
  0 has no indicator because it is always active.
- While Games (layer 3) is active, `W`, `A`, `S`, and `D` are red. The
  left-thumb Backspace position is amber because its Games action is Space.
- While Magic is held, the five keys below the top key in each outer column
  form bottom-up battery bars for the corresponding half. Each segment is a
  20% band; green is normal, amber/red is low, and blue means charging.

The battery bars intentionally use five segments.

## Agent attention lighting

This repository also provides `rmk-attentiond`, a small local daemon that maps
Codex and Claude Code approval/input requests onto expiring F1-F3 lighting
overlays. See [RMK Agent Attention](docs/rmk-agent-attention.md) for behavior,
Claude hook configuration, and development commands.

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
