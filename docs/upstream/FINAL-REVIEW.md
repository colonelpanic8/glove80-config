# Final pre-publish review

Review basis: local `origin/main` at `1156f82b`, `fix/dfu-split-hash-announce` at `bb8a9b18`, and `shared-flash-pr` at `d6ee36bf`. This is a skeptical upstream review against `CONTRIBUTING.md` and `CLAUDE.md`; no branch was checked out.

## 1. `fix/dfu-split-hash-announce` — **SHIP-AFTER-FIXES**

The BLE diagnosis is correct. `BleSplitPeripheralDriver::write` delegates to TrouBLE notification delivery (`rmk/src/split/ble/peripheral.rs:110-125`), and the pinned TrouBLE implementation returns `Ok(())` without sending when the CCCD is not enabled. On the central side, subscription completes before `PeripheralManager::run` starts (`rmk/src/split/ble/central.rs:336-368`), and that manager sends an initial `ConnectionStatus` before starting its DFU check (`rmk/src/split/driver.rs:92-127`). Consequently, receiving a successfully decoded central message is a valid BLE readiness barrier. The revised code performs the read and write sequentially through one mutable driver, so it introduces no concurrent driver access (`fix/dfu-split-hash-announce:rmk/src/split/peripheral.rs:91-123`).

The commit is small—one file, 16 insertions/10 deletions—and its subject is good. Its body explains CCCD-before-notify ordering crisply. It would be even stronger to name the surprising mechanism explicitly: TrouBLE returns success when the peer is unsubscribed, so `.ok()` hides the drop.

Required before publishing:

1. Scope the timing change to BLE, or explicitly justify and test the serial behavior. The old proactive send applied to every `dfu_split` transport; the new `hash_announced` path waits for inbound traffic on every transport even though the code comment and commit message are BLE-specific (`rmk/src/split/peripheral.rs:81-117`). The minimal bug fix should preserve the immediate serial announcement and use the deferred path under `all(feature = "dfu_split", feature = "_ble")`.
2. Only set `hash_announced` after a successful write. Lines 111-117 discard the result and permanently suppress retries for that session even when the driver reports an error. Use `is_ok()` (or an equivalent match) to retain the retry opportunity.
3. Add a regression test around a fake split driver/test hash provider: no announcement before an inbound message, a read error does not announce, the first decoded inbound message produces exactly one proactive announcement, and a failed write remains retryable. Also retain/cover the immediate serial behavior if item 1 is followed. A short manual BLE trace showing subscribe → initial `ConnectionStatus` → hash notification would satisfy the hardware side of the regression.

One protocol nit worth resolving or documenting: if the first inbound message is `FirmwareHashQuery`, the new proactive response is sent before the normal query arm sends another response (`rmk/src/split/peripheral.rs:110-117,160-168`). The current central normally sends `ConnectionStatus` first, so this is not the normal BLE path, but a test should pin down whether two responses are acceptable.

## 2. `shared-flash-pr` — **HOLD**

I would not merge this API in its current form. The opt-in polish is a real improvement, and dependency hygiene is otherwise good: it reuses RMK's existing direct `embassy-sync`/`static_cell` dependencies and routes the mutex type through RMK rather than requiring downstream code to name `embassy-sync` (`rmk/Cargo.toml:13-39`; `rmk-macro/src/codegen/chip/flash.rs:78-103`). `shared_flash` implying `storage` and the matching macro feature are also sensible (`rmk/Cargo.toml:138-146`; `rmk-macro/Cargo.toml:22-30`). The blockers are API correctness and configuration behavior, not formatting.

Blocking issues:

1. The documented single-client rule is not enforced. The public free functions share independent global request and reply channels (`rmk/src/shared_flash.rs:70-74,172-223`). Two tasks can issue requests and receive each other's replies. Replace the free-function protocol with a uniquely acquired client handle (for example, `take()` returning a `FlashClient` whose operations require `&mut self`), so one in-flight request is enforced by the API rather than prose. This is the main design blocker.
2. The public API can exist without a service. `shared_flash` does not imply an nRF BLE feature (`rmk/Cargo.toml:145-146`); non-nRF code generation ignores it. `dfu_nrf` wins the code-generation branch and also omits the service (`rmk-macro/src/codegen/chip/flash.rs:49-77,78-107`). Finally, `expand_flash_init` returns early when TOML storage is disabled (`rmk-macro/src/codegen/chip/flash.rs:12-19`). In all of those configurations, callable `read`/`write`/`erase` helpers wait forever. Unsupported combinations need compile-time diagnostics, or initialization must be explicit and usable on every configuration where the module is public.
3. The window API is weaker than its safety language. `set_window` publishes a pair through two independent relaxed atomics and does not enforce “once, before use” (`rmk/src/shared_flash.rs:76-98`). The module-level claim that a bug cannot erase firmware/RMK storage is too strong: callers may register any range, including those regions (`rmk/src/shared_flash.rs:19-21,81-89`). Put the immutable window in the uniquely owned client/service initialization, validate it, and phrase the guarantee as containment after a correct non-overlapping partition is supplied.
4. The claimed overflow handling is incomplete. The erase loop computes `at + page` before clamping to `to` (`rmk/src/shared_flash.rs:138-157`); a caller-selected high window can overflow there rather than return `FlashOpError`. Use checked/saturating arithmetic and test the boundary. `SharedFlash::new` also silently records capacity zero in release mode if `try_lock` fails (`rmk/src/shared_flash.rs:234-241`); generated-code invariants should fail deterministically or avoid fallible discovery.
5. There are no tests of the new module or macro output. The added `async_matrix,shared_flash` matrix entry only compiles/runs the existing unrelated tests (`scripts/test_all.sh:24-39`). Add a fake `AsyncNorFlash` test suite covering chunking, window boundaries, overflow, driver-error propagation, per-page erase, and “no flash touched” failures; add macro/config coverage for supported nRF generation and every rejected combination.
6. Docs.rs will not build this module: the docs feature list omits `shared_flash` (`rmk/Cargo.toml:104-105`), while the new guide links directly to that module (`docs/docs/main/docs/features/storage.md:30-33`). Enable the feature for docs.rs or do not publish a link that will be absent. The guide must also state what happens for pure-Rust initialization and TOML storage-disabled builds.
7. Against the requested local `origin/main`, the branch contains the unrelated 284-line Nix flake commit `5feaf8b1`; it is part of the three-commit PR range and adds `.envrc`, `flake.nix`, and `flake.lock`. Rebase/drop it unless that exact commit is already on the actual upstream base at publication time. Do not ask a maintainer to review it as part of this feature.

Naming is broadly understandable, but `SharedFlash` currently means the internal mutex adapter while `shared_flash::{read,write,erase}` means the application client. If the client-handle redesign is made, reserve the most obvious name (`SharedFlash` or `SharedFlashClient`) for the user-facing object and mark generated integration types such as `FlashMutex`/the service entry point `#[doc(hidden)]` where practical. The two feature commits should also follow the repository's prevailing conventional style, e.g. `feat(storage): share nRF flash with application code`, rather than `flash: ...`.

## 3. Submission texts — **SHIP-AFTER-FIXES**

The tone is appropriately technical and restrained for a first contribution. `ISSUE-dfu-hash.md` and `PR-shared-flash.md` do not name Glove80 or reveal its implementation details; “a split keyboard port” and “application-owned runtime configuration partition” are generic motivation. Keep `PR-READINESS.md` private: it explicitly names `glove80-import/main` and the imported branch history (`PR-READINESS.md:25-28`). That is useful local provenance context, not submission copy.

### `ISSUE-dfu-hash.md`

Required edits:

- Add the concrete platform, RMK commit/version, Rust version, minimal `dfu_split` configuration, exact boot/reconnect steps, and observable log/result. `CONTRIBUTING.md:76-97` specifically asks for platform/version and a reproducible case; the current text supplies mechanism but not a reproducer.
- Remove the reference to “the local branch” (`ISSUE-dfu-hash.md:30-32`); upstream cannot inspect it. Say that a minimal fix is ready and link the PR once it exists.
- Qualify or remove “the central booted first and has already stopped waiting for a query response” (`ISSUE-dfu-hash.md:7-9`) unless the reproduction demonstrates that state. In the current normal BLE connection path, the central subscribes, starts the manager, sends `ConnectionStatus`, and immediately performs its DFU check (`rmk/src/split/ble/central.rs:364-368`; `rmk/src/split/driver.rs:114-127`). The silent-drop finding is sound, but that particular user-visible consequence is not established by the text.
- “Never reaches”/“reliably lost” (`ISSUE-dfu-hash.md:5-6,17-20`) is defensible for the pre-CCCD send, but tie it to a named tested target/session trace rather than presenting it as proven for every BLE controller/host.

### `PR-shared-flash.md`

Required edits before this body accompanies a reworked branch:

- Line 28 is stale: the module is exposed with `shared_flash`, which implies `storage`, not with `storage` alone (`rmk/src/lib.rs:87-89`; `rmk/Cargo.toml:138-146`).
- Lines 54-58 overstate final-head verification. `PR-READINESS.md:38-61` says the full matrix ran on the imported baseline, while the polished head only ran the dedicated 517-test feature set and two example checks (`PR-READINESS.md:63-74`). Either rerun `scripts/test_all.sh` on the final head and report the now-13 RMK configurations, or state the split verification history exactly.
- Remove the inaccessible `test-logs/shared-flash.log` reference (`PR-shared-flash.md:57-58`) unless the log will be attached as a CI artifact or gist.
- Lines 60-61 are stale/inaccurate against the reviewed refs: the actual polished head is `d6ee36bf`, and local `origin/main` is `1156f82b`; `5feaf8b1` is an extra child commit in the branch, not the reviewed `origin/main`. Prefer no ephemeral hashes in the PR body, or update them after the final rebase.
- Temper the design claim that interleaving prevents starvation (`rmk/src/shared_flash.rs:12-17`); releasing the mutex per page permits interleaving but does not by itself prove fairness.

## Final nits

- Run the prescribed formatter/full test script on each final rebased head and report final-head results only. Existing `git diff --check` is clean for both branches.
- For the DFU issue/PR, quote or link the pinned TrouBLE behavior rather than relying on a library-name assertion; the key fact is that an unsubscribed notification returns success without transmission.
- Keep implementation provenance records locally, but do not publish `PR-READINESS.md` or branch names containing `glove80-import`.

## Fixes applied

### `fix/dfu-split-hash-announce`

Final commit: `43f97053` (`fix(split): defer BLE DFU hash announcement until link is ready`).

- Required item 1: the deferred readiness barrier is gated by
  `all(dfu_split, _ble)` and serial split retains its immediate startup
  announcement — `43f97053`.
- Required item 2: `hash_announced` changes only after `write` returns success,
  so a failed notification is retried after the next decoded central message —
  `43f97053`.
- Required item 3: fake-driver/hash-provider regressions cover no BLE
  announcement before inbound traffic, read errors, exactly one successful
  proactive announcement, retry after write failure, and immediate serial
  behavior — `43f97053`.
- Protocol nit: a successful deferred response triggered by an initial
  `FirmwareHashQuery` satisfies that query, producing one response; a test
  pins this behavior — `43f97053`.
- Commit-message nit: the body now names the mechanism explicitly: pinned
  TrouBLE returns success for an unsubscribed notification without transmitting
  it — `43f97053`.

### `shared-flash-pr`

Final commits: `fffeb13d` (`feat(storage): share nRF flash with application
code`) and `6d2a872d` (`feat(storage): make shared flash access safe and
explicit`). The branch range is now exactly these two commits over `1156f82b`;
the unrelated Nix flake commit is absent.

- HOLD (a), client handle: `take(window).await` uniquely acquires the public
  `SharedFlash` client; operations require `&mut self`; generated mutex,
  service, storage adapter, and capacity helper are hidden from the API index —
  `6d2a872d`.
- HOLD (b), configuration behavior: macro generation emits direct diagnostics
  for TOML storage disabled, non-nRF52 hardware, BLE disabled, and `dfu_nrf`;
  pure-Rust users retain an explicit generic service initialization path —
  `6d2a872d`.
- HOLD (c), window and safety: initialization validates and freezes one
  erase-aligned window against actual capacity. Documentation now promises
  containment only given a correct non-overlapping partition — `6d2a872d`.
- HOLD (d), arithmetic/capacity: client and erase-loop address math is checked;
  zero-capacity storage adapter construction panics deterministically, while
  client initialization reports `OutOfBounds` without touching flash —
  `6d2a872d`.
- HOLD (e), tests: eight fake-`AsyncNorFlash` tests cover chunking, exact window
  boundaries, overflow, alignment, driver errors, page-by-page erase, duplicate
  clients, zero capacity, and no-touch failures. Macro tests cover accepted
  nRF52 BLE generation and every rejected combination — `6d2a872d`.
- HOLD (f), docs: docs.rs enables `shared_flash`; the storage guide uses the
  handle API, describes feature gating and pure-Rust initialization accurately,
  and avoids a fairness claim — `6d2a872d`.
- HOLD (g), rebase: final branch history is `1156f82b..fffeb13d..6d2a872d`;
  `5feaf8b1` is not in the PR range.
- HOLD (h), history style: both feature subjects use conventional
  `feat(storage): ...` form — `fffeb13d`, `6d2a872d`.

### Submission texts

- `ISSUE-dfu-hash.md` now contains the MoErgo Glove80 nRF52840 platform, RMK
  `1156f82b`, Rust 1.97.0, BLE split DFU features, flash/boot/trace steps, a
  session-scoped observation, and a pinned TrouBLE source link. Local-branch
  wording and the unproven central-timeout claim are removed — `c5a7394f`.
- The corrected `PR-shared-flash.md` body removes branch/log references, fixes
  feature/API names, tempers interleaving language, and reports only observed
  final-head validation. The sandbox denied writes to
  `/home/imalison/Projects/rmk-upstreaming/PR-shared-flash.md`; the complete
  replacement is saved at `/tmp/PR-shared-flash.md` and is therefore not in a
  commit.
- `PR-READINESS.md` was not modified.

### Final validation results

- `sh scripts/format_all.sh`: passed on both final branch trees.
- DFU fake-driver tests on Rust 1.97.0 with
  `RUSTFLAGS="-C link-self-contained=no"`: BLE 5/5 passed; serial 1/1 passed.
- Shared-flash fake driver tests: 8/8 passed.
- Macro/config tests: `shared_flash` 2/2 passed;
  `shared_flash,dfu_nrf` 2/2 passed. Because the sandbox has no network and the
  cached registry lacks `diff 0.1.13`, these in-crate unit tests were run after
  temporarily excluding unrelated `macrotest`/`trybuild` dev dependencies;
  the manifest was restored afterward.
- `cargo check --no-default-features --features async_matrix,shared_flash`:
  passed.
- docs.rs-equivalent feature documentation build including `shared_flash`:
  passed, with one pre-existing bare-URL warning in `pmw3610.rs`.
- Prescribed `/home/imalison/Projects/rmk-upstreaming/run-tests.sh`: could not
  start because the sandbox denied access to the Nix daemon socket. A direct
  Rust 1.97 fallback compiled all three `rmk-types` and all 13 RMK feature
  configurations. The `rmk-types` runs passed (19, 62, and 19 tests), but plain
  `cargo test` is not a valid nextest substitute: every RMK configuration
  reproduced the known shared-process mock-clock state failure in four of five
  `keyboard_bilateral_test` cases. The required final nextest matrix is
  therefore **not recorded as green and still must be run outside this
  sandbox**.

### Environment deviations

- The sandbox makes the repository's real `.git` directory read-only. All
  commits and branch rewrites above were created in a writable alternate Git
  directory attached to the same working tree. A bundle is produced at
  `/tmp/rmk-prepublish-fixes.bundle`; the original checkout remains on `main`,
  but its normal refs cannot be advanced from this session.
- This “Fixes applied” section is recorded by the final local documentation
  commit that contains it.
