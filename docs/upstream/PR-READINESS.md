# Shared-flash PR readiness

## Verdict

**Ready after polish** on local branch `shared-flash-pr`.

The imported implementation is directionally sound and well documented, but
it was not ready to submit unchanged:

- It replaced the nRF storage path and spawned the shared-flash task for every
  non-DFU nRF storage build. The polish adds an opt-in `shared_flash` feature in
  both `rmk` and `rmk-macro`, preserving existing behavior and RAM/task usage
  when the feature is disabled.
- Generated code referred directly to `::embassy_sync`, which is not a required
  direct dependency of RMK applications and caused `#[rmk_keyboard]` expansion
  to fail. The polish exposes an RMK-owned `FlashMutex` alias and uses it in
  generated code.
- Request/reply protocol internals and channel storage were unnecessarily
  public. The polish keeps only the application helpers and generated-code
  integration types public.
- The feature had no user documentation or dedicated matrix entry. The polish
  documents feature enablement, address-window safety, and alignment, and adds
  an `async_matrix,shared_flash` nextest configuration.

The branch contains upstream commit `5feaf8b1` (Nix flake development
environment, #970) before the shared-flash commit because the imported feature
branch was based on `glove80-import/main`; the shared-flash change itself and
its polish remain separate commits.

## Test results

The prescribed `nix shell nixpkgs#{gcc,lld,cargo-nextest}` wrapper could not run
unchanged in this sandbox: Nix cache writes were initially denied, and after
redirecting the cache the flake registry could not be resolved without network
access. Verification therefore used the same pinned Rust and linker flags with
the already-present Nix-store `cargo-nextest` and `lld` binaries directly.

### Imported branch baseline

Full matrix: **all green**.

- `rmk-types`: default 19/19, `host` 62/62, `steno` 19/19.
- `rmk` nextest feature sets:
  - `split,vial,async_matrix,_ble`: 548/548
  - `split,vial,async_matrix`: 526/526
  - `split,async_matrix`: 519/519
  - `split,async_matrix,_ble`: 537/537
  - `async_matrix,storage`: 517/517
  - `vial,storage`: 528/528
  - `vial,_ble`: 548/548
  - `passkey_entry`: 539/539
  - `split,vial,storage,passkey_entry`: 550/550
  - `vial,storage,steno`: 537/537
  - `split,vial,storage,async_matrix,_ble,steno`: 557/557
  - no default features: 515/515
- `rmk-types` doctests: default and `host` both passed (0 doctests present).

The export used cache-compatible patch versions `env_logger 0.11.9` and
`jiff 0.2.20` because the locked `jiff 0.2.32` archive was not cached and the
sandbox cannot reach crates.io. No source branch or tracked lockfile was
modified for this substitution.

### Polished branch

- `async_matrix,shared_flash`: 517/517 nextest tests passed.
- `examples/use_config/nrf52840_ble`: `cargo check` passed with
  `shared_flash` enabled.
- The same nRF52840 BLE example: `cargo check` passed with `shared_flash`
  disabled, confirming the original generated flash path remains valid.
- Rust formatting and `git diff --check` passed.

The example checks used Rust 1.97.0,
`RUSTFLAGS="-C link-self-contained=no"`, cached dependencies, and the local
Nix-store `lld`/`libclang` paths.
