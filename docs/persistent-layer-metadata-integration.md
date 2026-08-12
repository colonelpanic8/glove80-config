# Persistent layer metadata integration record

The RMK portion of persistent layer management is committed in the nested RMK
repository on topic branch `feat/persistent-layer-metadata`:

- topic commit: `05d4327ee0ccadeef37425d5ce6f9020274d351f`
- upstream/assembly base: `1a411da55bcaae80487ce76ca00351ef1aee32f5`
- verified assembled tree: `f25b03b4b9afff807e73630839e96949600c3d8e`

The topic adds the `GetLayerMetadata` and `SetLayerMetadata` Rynk commands,
fixed-capacity `{ occupied, name }` metadata, compiled layer-name defaults,
persistent storage, native API support, WASM bindings, protocol snapshots, and
focused config/protocol/storage tests. It is intentionally not committed onto
the generated `assembled` branch.

## Assembly entry

After the `patches/runtime-lighting-wake-layers.patch` entry, add:

```toml
[[entry]]
branch = "fork:feat/persistent-layer-metadata"
summary = "Device-backed logical layer occupancy and names"
fixup = "patches/persistent-layer-metadata-coherence.patch"
```

The local proof used `local:feat/persistent-layer-metadata` because the topic
was not pushed. Before publishing, push the topic branch and use the `fork:`
source above, then run the normal fork-fold update/build/publish procedure.

The proof build recorded nine rerere resolutions:

| Rerere hash | Conflict path |
| --- | --- |
| `17d100df383db12425096a159e159b5597c221d0` | `docs/docs/main/docs/development/rynk_protocol.md` |
| `57b5277a6a43ab695e6b1d9a9751eb7cedc5148c` | `rmk-types/src/protocol/rynk/tests.rs` |
| `58f8921927c1d59bd90f6049123c1f7ae9c30263` | `rmk-types/src/protocol/rynk/snapshots/wire_values.snap` |
| `5f0f88cf113ea8025faa8c08c097663e2bd9c282` | `rynk/rynk-wasm/src/client.rs` |
| `930c55d3c8f054d7e0ea6bce8cfb7b9cedecb2ea` | `rmk-types/src/protocol/rynk/snapshots/wire_frames.snap` |
| `96b2594f48a2b9bf4b1bfb60cbb7fd7eb2da3c36` | `rmk/src/storage/mod.rs` |
| `a331ff673075c25e25baa4ebccc1d2e8d48d149c` | `rmk/src/host/rynk/mod.rs` |
| `ae495a14909b44a19363288596ef4feca3add5fd` | `rmk-types/src/protocol/rynk/command.rs` |
| `be415d3b4d8fe301e5743b9a6d0b6512a945a958` | `rynk/src/api.rs` |

## Required coherence fixup

`patches/persistent-layer-metadata-coherence.patch` must contain two stack-only
adaptations:

1. Add `layer_names: Vec::new()` to the standalone `Keymap` initializer in
   `rmk-config/src/resolved/lighting.rs`.
2. Expose the already-assembled pointing endpoints in
   `rynk/rynk-wasm/src/client.rs`:

   ```rust
   get_pointing_config() -> PointingConfig,
   set_pointing_config(config: PointingConfig) -> PointingConfig,
   ```

The second adaptation is required so the configurator can read, remap, write,
and verify pointing layer overrides during a lossless layer transaction.

The exact locked rebuild reproduced tree
`f25b03b4b9afff807e73630839e96949600c3d8e`. Generated commit IDs are not an
integration invariant and must not be used as the durable reference.

## Firmware proof

Both target bundles were built with Cargo 1.97.0, the repository board TOMLs,
and the verified assembled tree. Compared with the previous local known-good
artifacts, the address ranges were:

| Target | Previous | Persistent metadata build |
| --- | --- | --- |
| Glove80 left | `0x26000-0xd4700` | `0x26000-0xd5000` |
| Glove80 right | `0x26000-0x8a400` | `0x26000-0x8a400` |
| Go60 left | `0x26000-0xd7b00` | `0x26000-0xd8300` |
| Go60 right | `0x26000-0x8d100` | `0x26000-0x8d000` |

The family IDs remained `0x9807B007`/`0x9808B007` for Glove80 and
`0x9809B007`/`0x980AB007` for Go60. No firmware was flashed.
