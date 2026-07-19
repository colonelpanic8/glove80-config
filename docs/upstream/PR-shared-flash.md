# Storage: share the radio-safe flash driver with application storage

## Motivation

On nRF BLE targets, `nrf_mpsl::Flash` is the radio-safe flash implementation
and is a singleton. RMK's generated main currently gives that singleton
exclusively to its storage task. A keyboard application with a separate
reserved partition therefore cannot safely persist runtime settings without
forking generated initialization code or bypassing MPSL scheduling.

This was developed for a split keyboard port with an application-owned runtime
configuration partition.

## Design

- Generated nRF52 BLE/non-`dfu_nrf` initialization wraps the singleton flash
  driver in an Embassy async mutex.
- RMK storage receives a locking `AsyncNorFlash` adapter. The unique
  application-side `SharedFlash` client is acquired with `take(window).await`,
  and all operations require `&mut self`, enforcing one request in flight.
- Client initialization validates and freezes the erase-page-aligned address
  window against actual flash capacity before any flash operation is allowed.
- Reads and writes are chunked at 256 bytes. Multi-page erase releases and
  reacquires the lock once per erase page, permitting RMK storage traffic to
  interleave without claiming scheduling fairness.
- Checked address arithmetic, window checks, and alignment checks fail before
  touching the driver. Driver errors are returned to the client.
- `shared_flash` exposes the module and implies `storage`; `storage` alone does
  not expose it.
- Generated configurations fail at compile time when TOML storage is disabled,
  the chip is not nRF52, BLE is disabled, or `dfu_nrf` conflicts. Pure-Rust
  initialization can explicitly construct the mutex/storage adapter and spawn
  the generic service.

Given a correctly allocated application partition, every application operation
is contained within it. RMK cannot infer whether the supplied partition
overlaps firmware, the bootloader, or RMK storage, so partition correctness
remains the application's responsibility.

## Usage

Acquire the unique client from the task that owns the application partition:

```rust
use rmk::shared_flash::take;

let mut flash = take(PARTITION_START..PARTITION_END).await?;
flash.read(PARTITION_START, &mut header).await?;
flash.erase(PARTITION_START, PARTITION_END).await?;
flash.write(PARTITION_START, &data).await?;
```

Addresses are absolute. The window and operations must obey the underlying
flash alignment requirements.

## Testing

Observed on Rust 1.97.0 with `RUSTFLAGS="-C link-self-contained=no"`:

- 8 fake-`AsyncNorFlash` tests pass, covering chunking, window boundaries,
  overflow, alignment, driver-error propagation, per-page erase, duplicate
  acquisition, zero capacity, and failures that do not touch flash.
- Macro tests pass with `shared_flash` and with `shared_flash,dfu_nrf`, covering
  accepted nRF52 BLE generation and rejected storage-disabled, non-nRF52,
  BLE-disabled, and DFU-conflict configurations.
- The `async_matrix,shared_flash` production check and the docs.rs feature-set
  documentation build pass.

The final `scripts/test_all.sh` nextest matrix could not start in the isolated
review environment because access to the Nix daemon was denied. A direct
`cargo test` fallback compiled all 13 RMK feature combinations, but it is not a
valid replacement for nextest: the shared-process runner reproduced the known
mock-clock state failures in four bilateral tests. The exact nextest matrix
still needs to run in the normal development environment before publication.
