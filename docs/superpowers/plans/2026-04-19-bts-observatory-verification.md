# BTS Observatory — Verification Results

Branch: feat/bts-observatory
Final commit: 7454e9fcd0cfb5e46bee95def4c7b23ccd58faeb

## Build
- `cargo check -p rayhunter -p rayhunter-daemon -p rayhunter-check -p telcom-parser`: PASS

## Tests
- `rayhunter` lib (unit tests): 41 passed
- `rayhunter` lib (cell_observer_end_to_end integration test): 1 passed
- `rayhunter` lib (test_lte_parsing integration test): 1 passed
- `rayhunter-daemon` lib: 14 passed (no pre-existing flakes observed on this run)
- `rayhunter-daemon` bin: 20 passed
- `telcom-parser` lib: 0 tests (no unit tests in crate)
- `telcom-parser` integration (lte_rrc_test): 1 passed
- `cell_observer_end_to_end` integration test: PASS

Note: The spec anticipated up to 3 pre-existing `notifications::test_notification_worker_*`
failures (TLS CryptoProvider Once poisoning on concurrent test threads). All notification tests
passed cleanly on this run — no flakes observed.

## Clippy
- Workspace (relevant crates): PASS — 0 warnings, 0 errors

  Three pre-existing clippy issues were fixed as part of Task 27:
  - `daemon/src/notifications.rs:296`: `assert!(false)` replaced with `panic!()`
  - `daemon/src/analysis.rs`: unused `cell_store` struct field and `cell_store()` accessor removed
    (the store is accessible via `Harness::get_store()`; the redundant copy in `AnalysisWriter` was
    added during earlier task work and never wired up)
  - `lib/src/cell/store.rs:377`: `vec![...]` in a test replaced with an array literal

## Frontend
- `npm run check`: 0 errors, 0 warnings
- `npm run build`: success (site written to `daemon/web/build`)

## Firmware profile
- `cargo build-daemon-firmware-devel`: PASS — `firmware-devel` profile built successfully
  (armv7 musl cross-toolchain is present on this machine)

## Remaining out-of-scope / follow-ups (from spec §16)
- Qualcomm ML1 parser for real-time serving cell RSRP
- Global cross-recording cell DB
- 5G NR cell listing
- Cell location estimation
- Operator override file
- REPLAY time-slider
