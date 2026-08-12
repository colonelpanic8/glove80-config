# Persistent layer metadata integration record

The RMK implementation lives on a normal nested-repository topic branch, not
on generated `assembled` output:

- branch: `feat/persistent-layer-metadata`
- topic commit: `05d4327ee0ccadeef37425d5ce6f9020274d351f`
- upstream/assembly base: `1a411da55bcaae80487ce76ca00351ef1aee32f5`

The topic adds `GetLayerMetadata` and `SetLayerMetadata`, fixed-capacity
`{ occupied, name }` slots, compiled defaults, flash persistence, native and
WASM APIs, wire snapshots, and focused protocol/config/storage tests. It was
not pushed as part of this task.

## Generated-stack qualification

The exact locally generated tree used for qualification is:

- generated commit: `79a8f38d6082c98abf8b140875e3347b222bd62d`
- generated tree: `b1cd6378ffa76655f6ce43c845f9083da393e2f1`
- MoErgo integration branch: `feat/layer-metadata-integration`
- MoErgo integration head: `46c3c31`

The proof assembly used a temporary local remote and admitted the topic before
the existing carried stack. Its final coherence resolution retained:

1. the layer metadata rows in the final Rynk documentation, wire tests, and
   regenerated snapshots;
2. `layer_names` initialization in the standalone lighting `Keymap`
   constructor introduced later in the stack;
3. `wake_layers` in standard and split-replica state, including snapshot
   capture and application;
4. the existing pointing-config WASM endpoints needed to rewrite pointing
   layer overrides losslessly.

The protocol/configuration fixup was captured at the final feature-branch
boundary (`feat/device-data`); the wake-layer fields were retained while
resolving the generated runtime-lighting patch; and the pointing bindings were
applied as the final generated patch entry. These are stack-integration
adaptations, not changes to hand-commit on `assembled`.

To publish later:

1. Push `feat/persistent-layer-metadata` to the writable RMK fork.
2. Replace the temporary `local:` assembly source with
   `fork:feat/persistent-layer-metadata`.
3. Carry the recorded coherence fixups/resolutions through the assembly
   repository and run its normal update/build/publish procedure.
4. Update downstream pins only to the generated commit. Never hand-commit to
   `assembled`.

## Verification record

The final generated tree passed:

- all 110 host-enabled `rmk-types` tests, including final wire snapshots;
- all 119 `rmk-config` unit/integration tests;
- the `layer_metadata_survives_flash_map_reopen` nextest persistence test;
- MoErgo `just check`, including both runtime/compiled configuration models;
- the exact Rynk WASM Nix build used by Rynkbench;
- Rynkbench TypeScript, 253 UI/unit tests, and lint (only existing Fast Refresh
  warnings remain).

Both peer firmware bundles also built and passed UF2 validation:

| Target | Address range | Family ID |
| --- | --- | --- |
| Glove80 left | `0x26000-0xd5c00` | `0x9807b007` |
| Glove80 right | `0x26000-0x8a900` | `0x9808b007` |
| Go60 left | `0x26000-0xd9700` | `0x9809b007` |
| Go60 right | `0x26000-0x8de00` | `0x980ab007` |

No firmware was flashed.
