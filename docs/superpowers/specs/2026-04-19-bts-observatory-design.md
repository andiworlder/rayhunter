# BTS Observatory — Cell Tower Listing & Alert Enrichment

**Date:** 2026-04-19
**Status:** Approved
**Target repo:** EFForg/rayhunter (monorepo, Rust workspace + Svelte)

## 1. Overview

Add a new capability to Rayhunter that lists all base stations (BTS / cell towers) observed during a recording, with their signal strength measurements, and enriches every analyzer alert with the BTS context in which it occurred.

Out of the box today Rayhunter captures RRC/NAS traffic and emits severity events when an analyzer fires, but it does not surface which cell the device was attached to or which cells were visible nearby. This feature closes that gap.

The feature exposes a dedicated live-monitoring dashboard in the web UI (modeled on the mockup provided by the user) that works in two modes: **LIVE** during an active recording and **REPLAY** for any past recording.

## 2. Goals

- List every serving and neighbor cell observed during a recording, keyed by radio access technology.
- Record aggregated signal metrics per unique cell (min, max, last, avg).
- Attach a `cell_context` snapshot (serving cell + visible neighbors) to every `Event` emitted by an analyzer.
- Provide live streaming of cell state during an active recording with ~2 second polling cadence.
- Provide replay view of cell history for past recordings.
- Resolve PLMN (MCC + MNC) to operator name via bundled offline lookup.
- Remain self-contained: no new external services, no internet requirement.

## 3. Non-Goals (Phase 1)

- Qualcomm ML1 binary log parsing (logged as Phase 2 follow-up).
- Cross-recording global cell database / SQL store.
- Cell location estimation (lat/lon).
- 5G NR cell listing (ASN.1 decoder not present in `telcom-parser`).
- Historical timeline navigation slider in REPLAY mode.
- User-editable operator override file.

## 4. Scope

### 4.1 Radio Access Technologies

- **LTE (full)** — serving identity from SIB1; neighbors from `MeasurementReport` (UL-DCCH). Full decoder path already present via `telcom-parser`.
- **UMTS (3G) — scaffolding only in Phase 1.** `InformationElement::UMTS` is currently a unit variant (`lib/src/analysis/information_element.rs:24`). This plan extends it to carry GSMTAP metadata (ARFCN, signal strength from GSMTAP header) so that UMTS packets contribute at least RAT identification and signal strength. Full ASN.1 decoding of UMTS RRC is deferred to a follow-up.
- **GSM (2G) — scaffolding only in Phase 1.** Same approach: `InformationElement::GSM` extended to carry GSMTAP metadata (ARFCN, signal_dbm, `frame_number`). Full decoding of System Information and Measurement Report in GSM RR is deferred.

Phase 1 therefore yields: LTE = full cell identity + neighbors + signal; GSM/UMTS = one row per unique (ARFCN, RAT) with signal from GSMTAP header, labeled clearly as "limited" in the UI.

### 4.2 Data Granularity

- **Aggregated per-recording.** One entry per unique cell (key: `rat + pci_or_psc_or_bsic + earfcn_or_uarfcn_or_arfcn [+ cid + tac + plmn when known]`) per recording.
- Each entry records: `first_seen`, `last_seen`, `observation_count`, signal min/max/avg, `is_serving` flag, and per-source `last_signal` values.

### 4.3 Signal Source (Phase 1)

- Serving cell identity: always available from SIB1/SIB3 broadcast (LTE/UMTS) or SI3 (GSM).
- Serving cell signal: populated **opportunistically** from MeasurementReport's `servingCellMeasResult` when UE reports it. Empty otherwise — UI displays "measuring…".
- Neighbor cell signal: always populated when MeasurementReport is seen.

### 4.4 Alert Enrichment

Every `Event` gets an optional `cell_context` snapshot that captures the state at emission time:

- `serving`: the currently active serving cell identity + last known signal.
- `neighbors[]`: up to 8 strongest neighbor cells with signal.

Snapshot is taken at the instant `analyze_information_element()` returns `Some(Event)`.

## 5. Architecture

```
  /dev/diag  (QMDL bytes)
      │
      ▼
  DiagDevice  (existing, lib/src/diag_device.rs)
      │  GsmtapMessage stream
      ▼
  gsmtap_parser  (existing)
      │  InformationElement
      ▼
  ┌────────────────────────────────────────────────────────┐
  │  Harness (existing, lib/src/analysis/analyzer.rs)       │
  │   ├── CellObserver  (NEW) ────► CellStore (NEW)         │
  │   │     extracts cell IEs      │  live buffer + aggregator│
  │   │                            │  per-recording persist  │
  │   └── Analyzer[] ◄──── snapshot │                        │
  │         (existing) cell_context                          │
  └────────────────────────────────────────────────────────┘
      │
      ▼
  AnalysisRow ────► {name}.ndjson
  CellEvent   ────► {name}.cells.ndjson
      │
      ▼
  HTTP API (new routes in daemon/src/cells.rs)
      │
      ▼
  Svelte /cells dashboard
```

## 6. Data Model

### 6.1 `CellIdentity`

Shared identity type spanning all RATs. Union shape with RAT discriminator:

```rust
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Hash)]
#[serde(tag = "rat")]
pub enum CellIdentity {
    Lte {
        plmn: Option<Plmn>,        // MCC-MNC
        tac: Option<u32>,
        cid: Option<u64>,          // 28-bit E-UTRAN cell ID
        pci: Option<u16>,          // 0..503
        earfcn: Option<u32>,
    },
    Umts {
        plmn: Option<Plmn>,
        lac: Option<u16>,
        cid: Option<u32>,          // UTRAN cell ID
        psc: Option<u16>,          // primary scrambling code 0..511
        uarfcn: Option<u32>,
    },
    Gsm {
        plmn: Option<Plmn>,
        lac: Option<u16>,
        cid: Option<u32>,
        bsic: Option<u8>,          // base station identity code
        arfcn: Option<u16>,
    },
}
```

### 6.2 `SignalSample`

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SignalSample {
    pub timestamp: DateTime<FixedOffset>,
    pub rsrp_dbm: Option<i16>,    // LTE
    pub rsrq_db: Option<i8>,      // LTE
    pub rscp_dbm: Option<i16>,    // UMTS
    pub ecno_db: Option<i8>,      // UMTS
    pub rxlev: Option<u8>,        // GSM 0..63
}
```

Per-RAT metric presence:

- LTE: `rsrp_dbm`, `rsrq_db`
- UMTS: `rscp_dbm`, `ecno_db`
- GSM: `rxlev`

### 6.3 `CellAggregate`

Per-recording aggregated row:

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CellAggregate {
    pub identity: CellIdentity,
    pub first_seen: DateTime<FixedOffset>,
    pub last_seen: DateTime<FixedOffset>,
    pub observation_count: u32,
    pub is_serving_ever: bool,
    pub current_signal: Option<SignalSample>,
    pub signal_min: Option<SignalSample>,
    pub signal_max: Option<SignalSample>,
    pub signal_avg_rsrp_dbm: Option<i16>,
    pub signal_avg_rxlev: Option<u8>,
    pub operator_name: Option<String>,   // resolved from PLMN
    pub flags: Vec<CellFlag>,            // e.g. NotInNeighborList
}
```

### 6.4 `CellContext` (alert enrichment)

Attached to `Event.cell_context`:

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CellContext {
    pub serving: Option<CellIdentity>,
    pub serving_signal: Option<SignalSample>,
    pub neighbors: Vec<NeighborSnapshot>,  // up to 8, strongest first
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NeighborSnapshot {
    pub identity: CellIdentity,  // minimal — PCI/PSC/BSIC + freq
    pub signal: SignalSample,
}
```

### 6.5 Persisted Report Format

File `{recording_name}.cells.ndjson` (NDJSON lines):

- Line 1: `CellReportMetadata { report_version, rayhunter_version, created_at }`
- Line 2+: `CellEvent` per batch (every 10s flush OR on recording close):
  ```json
  { "flushed_at": "…", "aggregates": [ CellAggregate, … ] }
  ```

Aggregates are **rewrite-style**: each flush contains the complete current aggregation state. Reader of the file takes the last line to get final state.

### 6.6 Event Schema Extension

`lib/src/analysis/analyzer.rs`:

```rust
pub struct Event {
    pub event_type: EventType,
    pub message: String,
    pub cell_context: Option<CellContext>,   // NEW
}
```

Backward-compatible: missing field deserializes to `None`. Bump `REPORT_VERSION` from current value to `+1`.

## 7. IE Extraction

New module: `lib/src/analysis/cell_observer.rs`.

### 7.1 LTE

Extract from `InformationElement::LTE`:

- **`BcchDlSch::SystemInformationBlockType1`** — `cell_access_related_info.{plmn_identity_list, tracking_area_code, cell_identity}`. Pattern already proven in [test_analyzer.rs](lib/src/analysis/test_analyzer.rs).
- **`DlDcch::RrcConnectionReconfiguration` carrying `measObjectEUTRA`** — inter/intra-freq neighbor list config (EARFCN enumeration).
- **`UlDcch::MeasurementReport`** — `measResults.measResultServCell` (RSRP/RSRQ for serving) + `measResultNeighCells.measResultListEUTRA[].{physCellId, measResult.{rsrpResult, rsrqResult}}`. RSRP quantization: `rsrpResult − 140` = dBm. RSRQ: `(rsrqResult − 40) / 2` = dB.
- **`UlCcch::RrcConnectionRequest`** — weak signal at request; optional.

PCI not in SIB1 — only in ML1 logs (Phase 2). Phase 1 records PCI only from MeasurementReport (where UE reports it for serving cell too).

### 7.2 UMTS (3G)

WCDMA IEs flow via GSMTAP subtype. Extract from UMTS RRC:

- **`MasterInformationBlock` + `SystemInformationBlockType3`** — CID, LAC, PLMN.
- **`MeasurementReport`** — intra-frequency measurements: PSC, RSCP, Ec/No.

Requires extending `InformationElement` enum to include `Umts(UmtsIe)` variant if not present. Check `lib/src/analysis/information_element.rs` and extend parser in `gsmtap_parser.rs`.

### 7.3 GSM (2G)

- **System Information Type 3** — CID, LAC, PLMN, BSIC, ARFCN.
- **Measurement Report** (RR) — 6-neighbor RxLev list.

Extend `InformationElement` if GSM variant not already present.

### 7.4 Skipped Messages

When a RAT's decoder is not yet present, skip cleanly with a `skipped_message_reason` — no panic, no log spam. This preserves analyzer behavior for partial coverage.

## 8. CellStore

New module: `lib/src/cell_store.rs`.

### 8.1 Responsibilities

- Receive `CellObservation` events from `CellObserver`.
- Maintain `HashMap<CellKey, CellAggregate>` for current recording. See 8.4 for key + merge.
- Maintain ring buffer of `SignalSample` for serving cell (last 120 samples, sufficient for ~4 min chart at 2s polling).
- Maintain ring buffer of neighbor count per-timestamp.
- Serialize snapshot for `CellContext` on demand.
- Flush to `{name}.cells.ndjson` every 10 s of wall time and on shutdown / recording close.
- Reset on new recording start.

### 8.2 Concurrency

`CellStore` lives in an `Arc<RwLock<CellStore>>` shared between:
- The analysis harness (writer)
- The HTTP handlers in `daemon/src/cells.rs` (reader)

Reads take `.read()`, batch writes take `.write()`. No long-held locks. Ring buffers are `VecDeque<SignalSample>` with fixed capacity.

### 8.4 Deduplication Key & Merge Strategy

A cell may be observed multiple ways in one recording:

- As serving cell via SIB1/SIB3/SI3 — known identity: `(plmn, tac, cid)`.
- As neighbor in a MeasurementReport — known identity: `(pci/psc/bsic, freq)` only.
- Same PCI later becomes serving (handover) — now learn `(plmn, tac, cid)` for that PCI.

`CellKey` is a tuple built per-RAT:

```rust
enum CellKey {
    Lte { plmn: Option<Plmn>, cid: Option<u64>, pci: Option<u16>, earfcn: Option<u32> },
    Umts { plmn: Option<Plmn>, cid: Option<u32>, psc: Option<u16>, uarfcn: Option<u32> },
    Gsm  { plmn: Option<Plmn>, cid: Option<u32>, bsic: Option<u8>, arfcn: Option<u16> },
}
```

Lookup order on insert:
1. If incoming identity has `cid` + `plmn`: merge into entry keyed by `(plmn, cid)` if present, else insert.
2. Else (neighbor-only): look up by `(pci/psc/bsic, freq)`. Merge or insert.
3. On handover that resolves neighbor → serving: promote the `(pci, freq)` entry by filling in `cid/plmn/tac`. Remove any duplicate `(plmn, cid)` entry created earlier and fold observation counts.

Merging preserves `first_seen` (min), updates `last_seen` (max), increments `observation_count`, and merges signal min/max/avg.

### 8.3 Alert Snapshot API

```rust
impl CellStore {
    pub fn current_context(&self, max_neighbors: usize) -> CellContext;
}
```

Invoked by harness immediately after each analyzer emits an `Event`. The returned context is moved into `Event.cell_context`.

## 9. HTTP API

New file: `daemon/src/cells.rs`. Register routes in [daemon/src/main.rs](daemon/src/main.rs).

### 9.1 `GET /api/cells/live`

Returns current in-memory state. Used by LIVE mode.

**Response 200:**
```json
{
  "mode": "live",
  "recording_name": "1713524000",
  "serving": {
    "identity": { "rat": "Lte", "plmn": "510-10", "tac": 1280, "cid": 23401, "pci": 142, "earfcn": 1350 },
    "operator_name": "XL Axiata",
    "signal": { "timestamp": "2026-04-19T12:40:00+08:00", "rsrp_dbm": -79, "rsrq_db": -9 }
  },
  "neighbors": [
    { "identity": { "rat": "Lte", "pci": 301, "cid": 23410, "earfcn": 3050 },
      "operator_name": "XL Axiata",
      "signal": { "rsrp_dbm": -83 } }
  ],
  "stats": {
    "total_cells_seen": 12,
    "rat_distribution": { "Lte": 11, "Gsm": 1 }
  },
  "timeseries": {
    "serving_rsrp": [ [ts1, -82], [ts2, -79], ... 120 points ],
    "neighbor_count": [ [ts1, 5], [ts2, 6], ... ]
  }
}
```

**Response 204** when no active recording.

### 9.2 `GET /api/cells/{recording_name}`

Returns aggregated cells from persisted file (REPLAY mode).

**Response 200:** list of `CellAggregate` (reads last line of `.cells.ndjson`).

**Response 404:** recording not found.

### 9.3 `GET /api/cells/{recording_name}/timeseries`

Returns decimated timeseries for chart rendering in REPLAY mode. Decimation target: ~120 points.

**Response 200:**
```json
{
  "serving_rsrp": [ ["2026-04-19T10:00:00+08:00", -82], ... ],
  "neighbor_count": [ ["2026-04-19T10:00:00+08:00", 5], ... ]
}
```

### 9.4 Existing endpoint change

`GET /api/analysis-report/{name}` — each `Event` now carries `cell_context` when present. No route change.

## 10. Operator Name Resolution

New file: `lib/src/mcc_mnc.rs` + `lib/data/mcc_mnc.csv`.

- Source: Wikipedia MCC/MNC list (public domain), filtered to columns `mcc,mnc,country,network`.
- Embed as `include_str!("../data/mcc_mnc.csv")` at compile time.
- Parse once at startup into `HashMap<(u16, u16), &'static str>` behind `OnceLock`.
- Function `pub fn lookup_operator(mcc: u16, mnc: u16) -> Option<&'static str>`.
- Size budget: ~60 KB uncompressed. Acceptable for armv7 musl static binary.

Download/prep script `scripts/update-mcc-mnc.sh` for future refreshes.

## 11. Frontend

### 11.1 Route & files

- Route: `/cells` — new Svelte page at `daemon/web/src/routes/cells/+page.svelte`.
- Nav link added in `+layout.svelte` (top bar, next to Logs icon).
- Components in `daemon/web/src/lib/components/`:
  - `ServingCellCard.svelte`
  - `NeighborCellsTable.svelte`
  - `SignalHistoryChart.svelte` (inline SVG sparkline, no chart library needed)
  - `AlertsStrip.svelte` (reused from new design, matches mockup's third card)
- State store: `daemon/web/src/lib/cells.svelte.ts`.

### 11.2 Mode switching

Page reads `$page.url.searchParams.get('recording')`:
- absent → LIVE mode, polls `/api/cells/live` every 2 s (configurable via selector).
- present → REPLAY mode, fetches `/api/cells/{name}` + `/api/cells/{name}/timeseries` once.

### 11.3 Visual specification

Matches the mockup provided by the user:

- Header: "RAYHUNTER MONITOR" + live badge + refresh rate selector.
- Serving Cell card:
  - Large CID + operator name.
  - Large RSRP value with quality badge (`EXCELLENT` / `GOOD` / `FAIR` / `POOR`).
  - Inline grid: TAC / PCI / EARFCN / PLMN.
  - Horizontal progress bars for RSRP and RSRQ.
- Neighbor Cells card:
  - Count badge.
  - Table: `# / PCI / CID / RSRP bar / dBm / signal-bars icon`.
  - Footer: strongest / weakest / frequency list.
- RSRP History chart — inline SVG sparkline, 120 points.
- Neighbor Count chart — inline SVG sparkline.
- Alerts strip (third card, rendered only when alerts exist in this recording) — chronological list with expand-to-show `cell_context`.

### 11.4 Signal quality thresholds (LTE RSRP)

| Band | RSRP dBm | Color |
|---|---|---|
| Excellent | ≥ −80 | green-700 |
| Good | −80 to −90 | green-500 |
| Fair | −90 to −100 | amber-500 |
| Poor | < −100 | red-500 |

GSM RxLev (0–63) and UMTS RSCP get equivalent banding.

## 12. Config

`daemon/src/config.rs`:

```rust
pub struct Config {
    // … existing fields …
    pub bts_observatory: BtsObservatoryConfig,
}

pub struct BtsObservatoryConfig {
    pub enabled: bool,                  // default: true
    pub live_ring_buffer_size: usize,   // default: 120
    pub flush_interval_seconds: u64,    // default: 10
    pub max_neighbors_in_context: usize,// default: 8
}
```

When `enabled = false`: `CellObserver` + `CellStore` not wired, `cell_context` always None, routes return 404.

## 13. Backward Compatibility

- `REPORT_VERSION` bumped; old reports missing `cell_context` deserialize with `cell_context = None`.
- Existing endpoints unchanged in route/shape; only `Event` JSON gains an optional field.
- `AnalysisRow` V1/V2 deserializers unchanged; add a new normalizer line for the new optional field (defaults to null).
- Old QMDL files without captured MeasurementReport messages simply produce empty neighbor lists — no breakage.

## 14. Testing

- **Unit:** `cell_observer.rs` parses canned IE inputs per RAT (fixtures in `lib/tests/fixtures/cells/`). Golden test for MCC/MNC resolver on known PLMNs.
- **Integration:** play a captured QMDL fixture through `Harness::analyze_qmdl_file`, assert `CellStore` state post-run.
- **API:** tokio test that writes a cells.ndjson fixture and exercises each endpoint.
- **Frontend:** Vitest test for `cells.svelte.ts` store transitions (live → replay).

## 15. Failure Modes

| Condition | Behavior |
|---|---|
| No `MeasurementReport` seen in recording | Serving signal empty, neighbors empty. UI shows "measuring…" |
| Recording deleted mid-flight | `/api/cells/live` returns 204; file handlers return 404. |
| Corrupt `cells.ndjson` line | Skip the line, log warn, continue with last-valid state. |
| MCC/MNC not in table | `operator_name = None`, UI falls back to showing raw PLMN. |
| Full disk | `CellStore.flush()` logs error, drops the batch, keeps serving in-memory. Matches existing QMDL-write behavior. |

## 16. Out-of-Scope Follow-ups

1. **Qualcomm ML1 parser** for real-time serving cell RSRP independent of UE's MeasurementReport cadence. Would add log codes `0xB193` (LTE Serving Cell Info), `0xB139` (Connected Mode Neigh Meas), `0x5134` (GSM L1 Burst), `0x4127` (UMTS L1 Cell Meas). Port struct layouts from SCAT (MIT).
2. **Global cross-recording cell DB** via SQLite, for surveillance pattern analysis.
3. **5G NR cell listing** — requires ASN.1 decoder generation for NR RRC.
4. **Cell location estimation** using OpenCellID offline dump (optional download).
5. **Operator override file** at `/data/rayhunter/operators.toml`.
6. **REPLAY time-slider** to scrub through recording.

## 17. File Manifest

### New files

- `lib/src/analysis/cell_observer.rs`
- `lib/src/cell_store.rs`
- `lib/src/mcc_mnc.rs`
- `lib/data/mcc_mnc.csv`
- `daemon/src/cells.rs`
- `daemon/web/src/routes/cells/+page.svelte`
- `daemon/web/src/lib/components/ServingCellCard.svelte`
- `daemon/web/src/lib/components/NeighborCellsTable.svelte`
- `daemon/web/src/lib/components/SignalHistoryChart.svelte`
- `daemon/web/src/lib/components/AlertsStrip.svelte`
- `daemon/web/src/lib/cells.svelte.ts`
- `scripts/update-mcc-mnc.sh`

### Modified files

- `lib/src/lib.rs` — pub-export `cell_store`, `mcc_mnc`.
- `lib/src/analysis/analyzer.rs` — add `Event.cell_context`, bump `REPORT_VERSION`.
- `lib/src/analysis/mod.rs` — register `cell_observer` module.
- `lib/src/analysis/information_element.rs` — add UMTS/GSM variants if missing.
- `lib/src/gsmtap_parser.rs` — route UMTS/GSM subtypes to proper decoders.
- `daemon/src/main.rs` — 3 new route registrations.
- `daemon/src/analysis.rs` — wire `CellStore` into harness, take context snapshot per event.
- `daemon/src/config.rs` — add `BtsObservatoryConfig`.
- `daemon/src/qmdl_store.rs` — announce new recording lifecycle hooks for CellStore reset.
- `daemon/web/src/routes/+layout.svelte` — nav link to `/cells`.

## 18. Estimated Complexity

- Backend Rust: ~1400 LOC (new) + ~120 LOC (edits).
- Frontend Svelte: ~700 LOC (new) + ~40 LOC (edits).
- Fixtures & tests: ~500 LOC.
- Total: ~2800 LOC.
- Rough estimate: 4–6 engineer-days for a developer familiar with the codebase.
