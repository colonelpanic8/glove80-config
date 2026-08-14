# MoErgo keyboard configuration

Ivan's source-controlled Glove80 and Go60 configuration, managed by the
current [`moergo-rmk`](https://github.com/colonelpanic8/moergo-rmk) firmware
through RMK's native Rynk protocol.

The repositories, shared firmware layer, configuration model, and primary
control entry point use MoErgo names, reflecting that both boards are
first-class targets. See [Repository naming](docs/repository-rename.md) for
the migration and compatibility policy.

Personal keymap and lighting policy live in this repository. Firmware hardware
support, reusable lighting/protocol machinery, the multi-board control CLI,
and release packaging live in the pinned `dependencies/moergo-rmk` submodule.
The firmware
build injects [`config/firmware.toml`](config/firmware.toml) through RMK's
external keyboard-configuration path; `moergo-rmk` contains no personal
lighting rules.

## Setup

```sh
just init
just check
```

`just init` initializes the firmware repository and its pinned RMK submodule.
`just check` builds the pinned control tool and validates both boards' runtime
TOML offline.
Nix supplies the Rust and native dependencies used by the control tool.

## Apply the keymap

Connect the keyboard over USB or BLE, then run:

```sh
just diff
just apply
```

`config/glove80.toml` is a bidirectional representation of managed runtime
state: Bluetooth advertising name, keymap layers, default layer, brightness/background, output mode,
durable layer scenes and policy, and the generic lighting-extension selection
and optional overlay. Rynk supplies extension effect and palette names without
knowing which effect pack implements them. The current PaletteFX pack includes
the key-reactive Crosshair effect and exposes all seven of its tuning controls
through the same generic parameter interface.

`just diff` compares that TOML with the connected keyboard. `just apply` writes
only differences and verifies the resulting state. Bluetooth-name templates,
keymaps/default layers, and durable lighting scenes survive reboot; other
lighting values are live state whose boot defaults come from firmware. Key
writes are not atomic across the entire keymap, while durable lighting scenes
are replaced atomically.

`bluetooth_name = "Glove80 {slot}"` expands `{slot}` to the active BLE slot's
one-based number, so the keyboard advertises as `Glove80 1`, `Glove80 2`, or
`Glove80 3`. The persistent template can also be managed directly with
`./bin/moergo-control connection name get|set`; it is limited to 16 UTF-8
bytes by the legacy BLE advertising payload.

To inspect or pull state in the other direction:

```sh
just show                         # print canonical live TOML
just pull                         # rewrite config/glove80.toml from the keyboard
./bin/moergo-control config pull /tmp/glove80.toml
```

Pull preserves existing layer IDs when it can parse the destination, but
rewrites the file in canonical TOML form and therefore does not preserve
comments.

### Layer names

Each `[[layer]]`'s `name` is persistent firmware state, not a file-local
label: `just diff` reports a name the keyboard disagrees with, `just apply`
writes it, and `just pull` records a rename made in Rynkbench or elsewhere.
Names are at most 32 UTF-8 bytes. `config/firmware.toml` carries the same
names as compiled defaults for a fresh or reset keyboard.

To read or set one without a whole configuration file:

```sh
./bin/moergo-control keymap name             # list every slot
./bin/moergo-control keymap name 3 Games     # rename layer 3
```

### Selector-addressed bindings

A layer's `keys` grid reads well as a whole layer and badly as "these six
keys". `[[layer.bind]]` entries address keys with the same selectors the
[lighting model](docs/lighting.md) uses, and apply over the grid:

```toml
[[layer]]
id = "magic"
name = "Magic"
keys = """…"""

# QK_BOOT at matrix row 3, column 0.
[[layer.bind]]
key = [3, 0]
action = "QK_BOOT"

# Every key in zone 1, which is all 80 per-key positions.
[[layer.bind]]
zone = 1
action = "KC_NO"
```

Binds apply in file order and the last one covering a key wins it, so a broad
selector can go first and specific corrections after it. A layer with no `keys`
starts transparent, which is what a layer written entirely as binds wants.
`key = [row, col]` resolves offline; `key = N`, `led = N`, `zone = N`, and
`all = true` are questions about the board, so `just check` only confirms they
are well formed and names them, while `just diff` and `just apply` resolve them
through the connected keyboard's topology. The keyboard stores a grid rather
than the selectors that described one, so `just pull` writes layers back as
`keys` alone.

This is runtime configuration only. The compiled `[[keymap.layer]]` grids in
[`config/firmware.toml`](config/firmware.toml) are parsed by RMK's
`keyboard.toml` schema and take no binds.

The Go60's managed runtime state lives in
[config/go60.toml](config/go60.toml). Runtime TOML declares its logical
5×14 matrix and carries the persisted policy for both pointing devices; the
current policy makes device 0 (the left trackpad) scroll and device 1 (the
right trackpad) move the cursor. Because both keyboards can be connected at
once, Go60 recipes require an explicit Rynk HID path:

    just go60-diff /dev/hidraw12
    just go60-apply /dev/hidraw12
    just go60-pull /dev/hidraw12

The HID number can change after reconnecting. Identify the Go60 Rynk
interface before applying rather than reusing a stale path.

The same configuration commands also accept the experimental JSON backup
format from the MoErgo Layout Editor:

```sh
./bin/moergo-control config validate layout.json
./bin/moergo-control config diff layout.json
./bin/moergo-control config apply layout.json
./bin/moergo-control config pull layout.json --format moergo-json
./bin/moergo-control config show --format moergo-json
```

JSON import manages the runtime keymap and default layer; the editor format has
no Rynk lighting state. Pulling over an existing editor JSON preserves its
identity, layer names, macros, combos, custom behavior definitions, and other
editor-owned sections. A binding with no faithful Rynk/editor equivalent is
rejected with its exact layer and key position rather than silently changed.
The Layout Editor itself describes JSON import/export as experimental, so the
schema may evolve.

For transport selection or any other CLI command, use the pinned wrapper:

```sh
./bin/moergo-control --usb keymap read --all
./bin/moergo-control --ble version
./bin/moergo-control --usb lighting caps
./bin/moergo-control --usb device-data
```

Run `./bin/moergo-control --help` for the complete interface. The existing
`./bin/glove80-control` path remains as a compatibility shim.

## Firmware

Build release firmware from the exact pinned product stack with:

```sh
just firmware
```

Artifacts are written under `dependencies/moergo-rmk/dist/`. The firmware's
compiled defaults currently match this keymap, while this repository remains
the editable source of truth for subsequent runtime changes.

The compact counterpart for the Go60 RMK port lives in
[`config/go60-firmware.toml`](config/go60-firmware.toml). Build both Go60
halves from that configuration with:

```sh
just go60-firmware
```

The bundle is written under `dependencies/moergo-rmk/dist/go60/`. The Go60
port automatically prefers the board's half-duplex UART/TRRS link between
halves and falls back to BLE between halves when the cable is absent. Host
communication remains independently selectable between USB and BLE. Hardware
qualification is still required.

The build embeds three independently checkable Git identities in the Rynk
firmware label: this configuration repository's commit, the pinned
`moergo-rmk` commit and board-crate semver, and the pinned RMK submodule's full
`git describe` identity. Rynk also reports RMK's structured semantic version.
The release manifest records the full configuration, product, and RMK commits.
A dirty working tree is marked in both places.

## Lighting controls and indicators

See [Lighting model](docs/lighting.md) for the topology, selector syntax,
matrix/zone regions, resolution behavior, and compositor order.

Lighting has a three-state output policy: always on, always off, or on only
while USB power is present. In plugged-in-only mode each half evaluates its
own VBUS independently; USB power does not need to be the selected transport.
The final hardware driver caps each color channel at 230/255 (about 90%).

- Hold the left-thumb Magic key to temporarily wake lighting and show the
  information view without changing the selected policy.
- Press `Magic+T` to cycle always on → always off → plugged-in only. `T`
  reports the selected policy in green, red, or blue respectively.
- Press `Magic+R` to toggle the maintenance lock. `R` is green while the lock
  is off and fully unattended configuration and firmware pushes are allowed,
  and red while the lock is engaged. This firmware defaults the lock to off.
- The Magic lighting controls use the familiar WASD cluster plus left/right
  arrows: `W`/`S` raise and lower overall brightness, `E` toggles PaletteFX,
  `T` cycles the lighting policy, left/right cycle effects backward/forward,
  and `C` cycles palettes. Brightness is yellow, palette cycling is magenta,
  effect cycling is white, and both toggles report their live state. PaletteFX
  starts off and toggles on at half brightness.
- While lighting is on, F-keys `F1` through `F5` show non-default layers 1
  through 5 in blue while active. Inactive layers are transparent/dark; layer
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

## Updating `moergo-rmk`

Update deliberately, inspect the upstream changes, and then commit the new
gitlink:

```sh
git submodule update --remote dependencies/moergo-rmk
git -C dependencies/moergo-rmk log --oneline --decorate ORIG_HEAD..HEAD
just check
git add dependencies/moergo-rmk
```

The submodule tracks upstream `master`, but ordinary clones and builds always
use the exact commit recorded by this repository.
