# Lighting model

The firmware owns the physical lighting topology and stock boot defaults; the
runtime configuration owns the durable personal policy. `config/firmware.toml`
declares stock keys, LED emitters, geometry, zones, and outputs.
`config/glove80.toml` declares the personal keymap, editable layer scenes,
conditional indicators, brightness, output policy, and PaletteFX settings
applied through Rynk.

## From a key selector to an LED

Every physical light has a stable LED ID. An emitter may also be associated
with a logical matrix key, physical coordinates, and zero or more named zones.
Rynk exposes that topology to host tools. During `config diff` or `config
apply`, `moergo-control` resolves readable selectors to one or more LED IDs:

| Runtime selector | Meaning |
| --- | --- |
| `led = 34` | One low-level emitter ID |
| `key = 0` | One numeric logical key ID |
| `key = [0, 0]` | The key at matrix row 0, column 0 |
| `zone = 1` | Every emitter in zone 1 |
| `all = true` | Every emitter advertised by the device |

Compiled firmware uses the same selector shapes inside a target, for example
`target = { key = [0, 0] }`.

The vocabulary is not lighting-only. The same selectors address keys through
`[[layer.key]]` in the runtime configuration, where one entry carries both what
a key does and how it looks: the selector lowers to matrix keys for the action
and to emitters for the color. `led = 34` then names the key that owns emitter
34, and `zone = 1` names every key in that zone. See
[Keys described in one place](../README.md#keys-described-in-one-place). The
compiled `[[keymap.layer]]` grids in `config/firmware.toml` are still
grid-only: that schema belongs to RMK's `keyboard.toml` parser rather than to
this repository's host tools.

One key can own multiple emitters, and some emitters need not belong to a key.
That is why configuration resolves through the topology instead of assuming
that a key index and an LED index are the same number.

The selector is preserved in the source TOML, but not in the keyboard's stored
scene table. The keyboard stores the resolved LED cells, so a live read or
`just pull` emits low-level `led` selectors. `just diff` resolves the source
again before comparison, making the two forms equivalent. Selector expansion
does not add permanent firmware RAM, but every resulting LED cell counts
against the runtime scene-table capacity.

## Glove80 coordinates and regions

The Glove80 exposes a 6×14 logical matrix with holes at `[0, 5]`, `[0, 8]`,
`[5, 5]`, and `[5, 8]`. Columns 0–6 belong to the left half and columns 7–13
to the right half. The thumb clusters use column 6 on the left and column 7 on
the right; their row values identify the six thumb keys. Physical geometry in
the firmware places those keys in the actual curved thumb fans, so spatial
effects use measured positions rather than treating matrix neighbors as
physical neighbors.

There is currently one formal named zone:

| Zone | Members |
| --- | --- |
| `1`, `per-key` | All 80 key-attached RGB emitters |

The two LED outputs are hardware routing regions, not selector zones: node 0
drives left-half LED IDs 0–39 and node 1 drives right-half IDs 40–79. Raw LED
IDs follow each half's electrical chain and therefore do not match matrix or
visual order.

Several visually meaningful areas are presently expressed as sets of matrix
keys rather than formal zones:

- the left and right outer columns form the five-segment battery bars;
- the left thumb cluster shows USB and BLE connection state;
- F1–F5 indicate active non-base layers;
- the Magic-layer WASD/arrow cluster exposes lighting controls;
- the Games and Paseo layers highlight their relevant keys.

Adding named zones such as `left-thumb`, `right-thumb`, or `function-row`
would make those reusable selectors for both lighting cells and key bindings.
Whole-row, whole-column, and attribute predicates are not configuration syntax
yet; today they must be written as individual matrix-key entries or
represented by a declared zone.

## Where a cell is written

The two scene tables can be written standalone or attached to a layer, and the
difference is only where the file says it:

| Form | Reads as |
| --- | --- |
| `[[lighting.scene]]` with `layer = N` | A durable cell in that layer's scene table |
| `[[lighting.conditional_scene]]` | A host-owned rule, conditions and all |
| `[[layer.key]]` with `color` | The same durable cell, next to the key's action |
| `[[layer.key.rule]]` with `when` | The same rule, with this layer's condition implied |
| `[[layer.light]]` / `[[layer.light.rule]]` | The same two, for emitters that belong to no key |

A key's rule arms read first-match-wins, with the inline `color` as the final
unconditional arm; conditional arms lower in reverse table order so the
device's later-cells-win composition agrees with that reading.

They differ in one behavior. The standalone scene table rejects two cells for
one slot, which is what catches a scene written twice. Layer-attached cells are
ordered instead: they apply over the standalone table and over each other in
the order written, so a broad selector followed by a specific correction reads
the way the compositor below already reads. Conditional rules stay ordered in
both forms, with the layer-attached ones after the standalone table.

## Composition and output

The renderer composes sources from lower to higher priority:

1. uniform background;
2. PaletteFX or another lighting extension;
3. compiled layer scenes, then runtime layer scenes;
4. transient host overlays;
5. compiled status rules, then runtime conditional scenes and live indicators.

Later matching cells win at the same priority. This is why broad rules can be
declared first and specific status colors later. The selected output policy
(`always-on`, `always-off`, or `powered-only`) and brightness are applied to
the composed result. The central half replicates the semantic lighting state
to the right half, and each half renders its local 40-LED output.

The current user-facing colors and controls are summarized in the
[Lighting controls and indicators](../README.md#lighting-controls-and-indicators)
section of the main README.
