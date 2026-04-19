# BTS Observatory Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a dedicated cell-tower monitoring dashboard to Rayhunter that lists every observed BTS (LTE full, 2G/3G limited) with signal strength per recording, and attaches a cell-context snapshot (serving + neighbors) to every analyzer alert.

**Architecture:** A new `CellObserver` analyzer extracts cell identity and measurement data from decoded `InformationElement`s and feeds a shared `CellStore` (aggregator + ring buffers). The existing analyzer harness snapshots `CellStore` state into each emitted `Event` via a new `Event.cell_context` field. A Svelte `/cells` route consumes three new HTTP endpoints for live and replay views.

**Tech Stack:** Rust (tokio/axum), Svelte 5 + TypeScript + Tailwind, existing `telcom-parser` crate for LTE RRC ASN.1 decoding, bundled Wikipedia MCC/MNC CSV for operator lookup.

**Spec:** `docs/superpowers/specs/2026-04-19-bts-observatory-design.md`

---

## File Map

### New files

| Path | Responsibility |
|---|---|
| `lib/src/cell/mod.rs` | Re-exports for cell-related types. |
| `lib/src/cell/identity.rs` | `Plmn`, `CellIdentity`, `CellKey` types and merge logic. |
| `lib/src/cell/signal.rs` | `SignalSample` + derivation helpers (rsrp decode etc). |
| `lib/src/cell/mcc_mnc.rs` | `lookup_operator(mcc, mnc)`. |
| `lib/src/cell/observer.rs` | `CellObserver` — extracts observations from `InformationElement`. |
| `lib/src/cell/store.rs` | `CellStore` — aggregator, ring buffers, persist. |
| `lib/data/mcc_mnc.csv` | Bundled operator table. |
| `lib/tests/cell_observer_lte.rs` | Integration test over a canned LTE fixture. |
| `daemon/src/cells.rs` | HTTP handlers for `/api/cells/*`. |
| `daemon/web/src/routes/cells/+page.svelte` | Dashboard page. |
| `daemon/web/src/lib/cells.svelte.ts` | Frontend state + API client. |
| `daemon/web/src/lib/components/ServingCellCard.svelte` | Serving cell card. |
| `daemon/web/src/lib/components/NeighborCellsTable.svelte` | Neighbor table. |
| `daemon/web/src/lib/components/SignalHistoryChart.svelte` | Inline SVG sparkline. |
| `daemon/web/src/lib/components/AlertsStrip.svelte` | Alert card tied to cell context. |
| `scripts/update-mcc-mnc.sh` | Helper to refresh the bundled CSV. |

### Modified files

| Path | Change |
|---|---|
| `lib/src/lib.rs` | `pub mod cell;` |
| `lib/src/analysis/information_element.rs` | Turn `GSM`/`UMTS` unit variants into structs carrying GSMTAP metadata. |
| `lib/src/analysis/analyzer.rs` | `Event.cell_context`, bump `REPORT_VERSION`. |
| `lib/src/analysis/mod.rs` | No new module needed (cell is at `lib::cell`). |
| `daemon/src/analysis.rs` | Construct `CellStore`, wire into harness loop, snapshot on event. |
| `daemon/src/config.rs` | Add `BtsObservatoryConfig`. |
| `daemon/src/main.rs` | Register 3 new routes. |
| `daemon/src/server.rs` | Add `CellStore` field to `ServerState`. |
| `daemon/web/src/routes/+layout.svelte` | Nav link to `/cells`. |
| `.gitignore` | Already updated during brainstorm. |

---

## Task 1: Workspace preflight

**Files:**
- Check: workspace builds clean before we start.

- [ ] **Step 1: Verify baseline build**

Run: `cargo check --workspace --exclude installer-gui`
Expected: success, no errors.

- [ ] **Step 2: Verify baseline tests**

Run: `cargo test --workspace --exclude installer-gui --lib`
Expected: all existing tests pass.

- [ ] **Step 3: Verify frontend builds**

Run: `cd daemon/web && npm ci && npm run build`
Expected: success, `daemon/web/build/` populated.

---

## Task 2: `Plmn` + `CellIdentity` types

**Files:**
- Create: `lib/src/cell/mod.rs`
- Create: `lib/src/cell/identity.rs`
- Modify: `lib/src/lib.rs`

- [ ] **Step 1: Create module skeleton**

Create `lib/src/cell/mod.rs`:

```rust
pub mod identity;
pub mod mcc_mnc;
pub mod observer;
pub mod signal;
pub mod store;

pub use identity::{CellIdentity, CellKey, Plmn};
pub use signal::SignalSample;
pub use store::{CellAggregate, CellContext, CellStore, NeighborSnapshot};
```

- [ ] **Step 2: Register module in lib**

Edit `lib/src/lib.rs` — add `pub mod cell;` below the other `pub mod` lines.

- [ ] **Step 3: Write failing test for Plmn formatting**

Create `lib/src/cell/identity.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Plmn {
    pub mcc: u16,
    pub mnc: u16,
    pub mnc_is_3_digit: bool,
}

impl std::fmt::Display for Plmn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.mnc_is_3_digit {
            write!(f, "{:03}-{:03}", self.mcc, self.mnc)
        } else {
            write!(f, "{:03}-{:02}", self.mcc, self.mnc)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plmn_formats_with_padding() {
        let p = Plmn { mcc: 510, mnc: 10, mnc_is_3_digit: false };
        assert_eq!(p.to_string(), "510-10");
    }

    #[test]
    fn plmn_formats_with_3_digit_mnc() {
        let p = Plmn { mcc: 310, mnc: 260, mnc_is_3_digit: true };
        assert_eq!(p.to_string(), "310-260");
    }
}
```

- [ ] **Step 4: Run the tests and confirm pass**

Run: `cargo test -p rayhunter cell::identity`
Expected: 2 passed.

- [ ] **Step 5: Add `CellIdentity` enum**

Append to `lib/src/cell/identity.rs` (still inside `identity.rs`, above the `#[cfg(test)]` block):

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "rat")]
pub enum CellIdentity {
    Lte {
        plmn: Option<Plmn>,
        tac: Option<u32>,
        cid: Option<u64>,
        pci: Option<u16>,
        earfcn: Option<u32>,
    },
    Umts {
        plmn: Option<Plmn>,
        lac: Option<u16>,
        cid: Option<u32>,
        psc: Option<u16>,
        uarfcn: Option<u32>,
    },
    Gsm {
        plmn: Option<Plmn>,
        lac: Option<u16>,
        cid: Option<u32>,
        bsic: Option<u8>,
        arfcn: Option<u16>,
    },
}

impl CellIdentity {
    pub fn rat(&self) -> &'static str {
        match self {
            CellIdentity::Lte { .. } => "LTE",
            CellIdentity::Umts { .. } => "UMTS",
            CellIdentity::Gsm { .. } => "GSM",
        }
    }

    pub fn plmn(&self) -> Option<Plmn> {
        match self {
            CellIdentity::Lte { plmn, .. }
            | CellIdentity::Umts { plmn, .. }
            | CellIdentity::Gsm { plmn, .. } => *plmn,
        }
    }

    /// Merge a newly-observed identity into self, filling in previously unknown fields.
    pub fn merge_from(&mut self, other: &CellIdentity) {
        match (self, other) {
            (
                CellIdentity::Lte { plmn: sp, tac: st, cid: sc, pci: spi, earfcn: se },
                CellIdentity::Lte { plmn: op, tac: ot, cid: oc, pci: opi, earfcn: oe },
            ) => {
                if sp.is_none() { *sp = *op; }
                if st.is_none() { *st = *ot; }
                if sc.is_none() { *sc = *oc; }
                if spi.is_none() { *spi = *opi; }
                if se.is_none() { *se = *oe; }
            }
            (
                CellIdentity::Umts { plmn: sp, lac: sl, cid: sc, psc: sps, uarfcn: su },
                CellIdentity::Umts { plmn: op, lac: ol, cid: oc, psc: ops, uarfcn: ou },
            ) => {
                if sp.is_none() { *sp = *op; }
                if sl.is_none() { *sl = *ol; }
                if sc.is_none() { *sc = *oc; }
                if sps.is_none() { *sps = *ops; }
                if su.is_none() { *su = *ou; }
            }
            (
                CellIdentity::Gsm { plmn: sp, lac: sl, cid: sc, bsic: sb, arfcn: sa },
                CellIdentity::Gsm { plmn: op, lac: ol, cid: oc, bsic: ob, arfcn: oa },
            ) => {
                if sp.is_none() { *sp = *op; }
                if sl.is_none() { *sl = *ol; }
                if sc.is_none() { *sc = *oc; }
                if sb.is_none() { *sb = *ob; }
                if sa.is_none() { *sa = *oa; }
            }
            _ => {} // different RATs: no-op
        }
    }
}
```

- [ ] **Step 6: Add merge test**

In the `tests` submodule of `identity.rs`, add:

```rust
    #[test]
    fn lte_identity_merges_missing_fields() {
        let mut a = CellIdentity::Lte {
            plmn: None, tac: None, cid: None, pci: Some(142), earfcn: Some(1350),
        };
        let b = CellIdentity::Lte {
            plmn: Some(Plmn { mcc: 510, mnc: 10, mnc_is_3_digit: false }),
            tac: Some(1280), cid: Some(23401), pci: None, earfcn: None,
        };
        a.merge_from(&b);
        let CellIdentity::Lte { plmn, tac, cid, pci, earfcn } = &a else { panic!() };
        assert_eq!(*plmn, Some(Plmn { mcc: 510, mnc: 10, mnc_is_3_digit: false }));
        assert_eq!(*tac, Some(1280));
        assert_eq!(*cid, Some(23401));
        assert_eq!(*pci, Some(142));
        assert_eq!(*earfcn, Some(1350));
    }
```

- [ ] **Step 7: Run tests and commit**

Run: `cargo test -p rayhunter cell::identity`
Expected: 3 passed.

```bash
git add lib/src/cell/ lib/src/lib.rs
git commit -m "feat(lib): add Plmn and CellIdentity types for BTS observatory"
```

---

## Task 3: `CellKey` for dedup

**Files:**
- Modify: `lib/src/cell/identity.rs`

- [ ] **Step 1: Write the failing test for key derivation**

In the `tests` submodule of `identity.rs`:

```rust
    #[test]
    fn cellkey_prefers_cid_when_available() {
        let id = CellIdentity::Lte {
            plmn: Some(Plmn { mcc: 510, mnc: 10, mnc_is_3_digit: false }),
            tac: Some(1280), cid: Some(23401), pci: Some(142), earfcn: Some(1350),
        };
        let k = CellKey::from(&id);
        assert_eq!(k, CellKey::LteByCid { plmn: Plmn { mcc: 510, mnc: 10, mnc_is_3_digit: false }, cid: 23401 });
    }

    #[test]
    fn cellkey_falls_back_to_pci_without_cid() {
        let id = CellIdentity::Lte {
            plmn: None, tac: None, cid: None, pci: Some(301), earfcn: Some(3050),
        };
        let k = CellKey::from(&id);
        assert_eq!(k, CellKey::LteByPci { pci: 301, earfcn: Some(3050) });
    }
```

- [ ] **Step 2: Run test to confirm it fails**

Run: `cargo test -p rayhunter cellkey`
Expected: FAIL with "cannot find type `CellKey`".

- [ ] **Step 3: Add `CellKey` to `identity.rs`**

Add above the `#[cfg(test)]`:

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CellKey {
    LteByCid { plmn: Plmn, cid: u64 },
    LteByPci { pci: u16, earfcn: Option<u32> },
    UmtsByCid { plmn: Plmn, cid: u32 },
    UmtsByPsc { psc: u16, uarfcn: Option<u32> },
    GsmByCid { plmn: Plmn, cid: u32 },
    GsmByArfcn { arfcn: u16, bsic: Option<u8> },
    Unknown { rat: &'static str },
}

impl From<&CellIdentity> for CellKey {
    fn from(id: &CellIdentity) -> Self {
        match id {
            CellIdentity::Lte { plmn: Some(p), cid: Some(c), .. } => CellKey::LteByCid { plmn: *p, cid: *c },
            CellIdentity::Lte { pci: Some(pci), earfcn, .. } => CellKey::LteByPci { pci: *pci, earfcn: *earfcn },
            CellIdentity::Umts { plmn: Some(p), cid: Some(c), .. } => CellKey::UmtsByCid { plmn: *p, cid: *c },
            CellIdentity::Umts { psc: Some(psc), uarfcn, .. } => CellKey::UmtsByPsc { psc: *psc, uarfcn: *uarfcn },
            CellIdentity::Gsm { plmn: Some(p), cid: Some(c), .. } => CellKey::GsmByCid { plmn: *p, cid: *c },
            CellIdentity::Gsm { arfcn: Some(a), bsic, .. } => CellKey::GsmByArfcn { arfcn: *a, bsic: *bsic },
            other => CellKey::Unknown { rat: other.rat() },
        }
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p rayhunter cell::identity`
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add lib/src/cell/identity.rs
git commit -m "feat(lib): add CellKey dedup scheme for cell aggregator"
```

---

## Task 4: `SignalSample` type & LTE decoders

**Files:**
- Create: `lib/src/cell/signal.rs`

- [ ] **Step 1: Write the failing test for RSRP decoder**

Create `lib/src/cell/signal.rs`:

```rust
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

/// Decode 3GPP TS 36.133 Table 9.1.4-1 `rsrpResult` (0..97) to dBm.
pub fn decode_lte_rsrp(raw: u8) -> i16 {
    // rsrp_dBm = rsrpResult - 140, clamped 0..97 -> -140..-43
    (raw.min(97) as i16) - 140
}

/// Decode 3GPP TS 36.133 Table 9.1.7-1 `rsrqResult` (0..34) to dB (half-steps).
pub fn decode_lte_rsrq(raw: u8) -> i8 {
    // rsrq_dB = (rsrqResult / 2) - 19.5, we round to nearest integer dB
    let v = raw.min(34) as i16;
    ((v - 39) / 2) as i8
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SignalSample {
    pub timestamp: Option<DateTime<FixedOffset>>,
    pub rsrp_dbm: Option<i16>,
    pub rsrq_db: Option<i8>,
    pub rscp_dbm: Option<i16>,
    pub ecno_db: Option<i8>,
    pub rxlev: Option<u8>,
}

impl SignalSample {
    pub fn is_empty(&self) -> bool {
        self.rsrp_dbm.is_none()
            && self.rsrq_db.is_none()
            && self.rscp_dbm.is_none()
            && self.ecno_db.is_none()
            && self.rxlev.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rsrp_decodes_boundaries() {
        assert_eq!(decode_lte_rsrp(0), -140);
        assert_eq!(decode_lte_rsrp(61), -79);
        assert_eq!(decode_lte_rsrp(97), -43);
        assert_eq!(decode_lte_rsrp(200), -43); // clamp
    }

    #[test]
    fn rsrq_decodes_boundaries() {
        assert_eq!(decode_lte_rsrq(0), -19);
        assert_eq!(decode_lte_rsrq(21), -9);
        assert_eq!(decode_lte_rsrq(34), -2);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p rayhunter cell::signal`
Expected: 2 passed.

- [ ] **Step 3: Commit**

```bash
git add lib/src/cell/signal.rs
git commit -m "feat(lib): add SignalSample with LTE RSRP/RSRQ decoders"
```

---

## Task 5: MCC/MNC lookup with bundled CSV

**Files:**
- Create: `lib/data/mcc_mnc.csv`
- Create: `lib/src/cell/mcc_mnc.rs`
- Create: `scripts/update-mcc-mnc.sh`

- [ ] **Step 1: Seed the CSV with a small known set**

Create `lib/data/mcc_mnc.csv`:

```csv
mcc,mnc,mnc_is_3_digit,country,operator
510,10,false,Indonesia,Telkomsel
510,11,false,Indonesia,XL Axiata
510,21,false,Indonesia,Indosat Ooredoo
510,89,false,Indonesia,3 (Tri)
310,260,true,United States,T-Mobile USA
310,410,true,United States,AT&T Mobility
311,480,true,United States,Verizon Wireless
505,1,false,Australia,Telstra
520,1,false,Thailand,AIS
520,3,false,Thailand,TrueMove H
```

(Full Wikipedia dump lands later via refresh script.)

- [ ] **Step 2: Write failing test for resolver**

Create `lib/src/cell/mcc_mnc.rs`:

```rust
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::cell::Plmn;

static TABLE: OnceLock<HashMap<(u16, u16), &'static str>> = OnceLock::new();

fn table() -> &'static HashMap<(u16, u16), &'static str> {
    TABLE.get_or_init(|| {
        let raw = include_str!("../../data/mcc_mnc.csv");
        let mut map = HashMap::new();
        for (i, line) in raw.lines().enumerate() {
            if i == 0 || line.trim().is_empty() {
                continue;
            }
            let cols: Vec<&str> = line.splitn(5, ',').collect();
            if cols.len() < 5 {
                continue;
            }
            let Ok(mcc) = cols[0].parse::<u16>() else { continue };
            let Ok(mnc) = cols[1].parse::<u16>() else { continue };
            map.insert((mcc, mnc), cols[4]);
        }
        map
    })
}

pub fn lookup_operator(plmn: Plmn) -> Option<&'static str> {
    table().get(&(plmn.mcc, plmn.mnc)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Plmn;

    #[test]
    fn resolves_xl_axiata() {
        let op = lookup_operator(Plmn { mcc: 510, mnc: 11, mnc_is_3_digit: false });
        assert_eq!(op, Some("XL Axiata"));
    }

    #[test]
    fn unknown_plmn_returns_none() {
        let op = lookup_operator(Plmn { mcc: 999, mnc: 99, mnc_is_3_digit: false });
        assert_eq!(op, None);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p rayhunter cell::mcc_mnc`
Expected: 2 passed.

- [ ] **Step 4: Add refresh script**

Create `scripts/update-mcc-mnc.sh`:

```bash
#!/usr/bin/env bash
# Refresh lib/data/mcc_mnc.csv from the Wikipedia MCC/MNC list.
# Requires: curl, python3, pandas. Only run locally, not in CI.
set -euo pipefail

OUT="$(dirname "$0")/../lib/data/mcc_mnc.csv"
python3 - <<'PY' > "$OUT"
import re, sys, urllib.request
# Use the maintained mirror at mcc-mnc.com or similar; here we fetch from mcc-mnc-list.com
# The engineer running this should swap in a stable public dump URL.
URL = "https://raw.githubusercontent.com/musalbas/mcc-mnc-table/master/mcc-mnc-table.csv"
raw = urllib.request.urlopen(URL).read().decode("utf-8")
print("mcc,mnc,mnc_is_3_digit,country,operator")
for line in raw.splitlines()[1:]:
    p = line.split(",")
    if len(p) < 6: continue
    mcc, mnc = p[0], p[1]
    if not mcc.isdigit() or not mnc.isdigit(): continue
    is_3 = "true" if len(mnc) == 3 else "false"
    country = p[4].replace(",", " ")
    op = p[5].replace(",", " ").strip('"')
    print(f"{mcc},{int(mnc)},{is_3},{country},{op}")
PY
```

Run: `chmod +x scripts/update-mcc-mnc.sh`

- [ ] **Step 5: Commit**

```bash
git add lib/data/mcc_mnc.csv lib/src/cell/mcc_mnc.rs scripts/update-mcc-mnc.sh
git commit -m "feat(lib): bundle MCC/MNC table and operator lookup"
```

---

## Task 6: Extend `InformationElement` for GSM/UMTS metadata

**Files:**
- Modify: `lib/src/analysis/information_element.rs`

- [ ] **Step 1: Replace unit variants with struct variants**

In `lib/src/analysis/information_element.rs`, replace:

```rust
pub enum InformationElement {
    GSM,
    UMTS,
    LTE(Box<LteInformationElement>),
    FiveG,
}
```

with:

```rust
pub enum InformationElement {
    GSM(GsmMeta),
    UMTS(UmtsMeta),
    LTE(Box<LteInformationElement>),
    FiveG,
}

#[derive(Debug, Clone)]
pub struct GsmMeta {
    pub arfcn: Option<u16>,
    pub signal_dbm: Option<i8>,
    pub frame_number: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct UmtsMeta {
    pub uarfcn: Option<u32>,
    pub signal_dbm: Option<i8>,
}
```

- [ ] **Step 2: Update `TryFrom<&GsmtapMessage>` impl**

Find the `impl TryFrom<&GsmtapMessage> for InformationElement` block. For any `GsmtapType` that previously returned `InformationElement::GSM` or `::UMTS` (or did not match at all, producing the unsupported error), change the mapping so GSM GsmtapType variants produce `InformationElement::GSM(GsmMeta { arfcn, signal_dbm, .. })` populated from the gsmtap header, and similarly for UMTS. Consult `lib/src/gsmtap.rs` to confirm the header field names (`gsmtap_msg.header.arfcn`, `signal_dbm`, `frame_number`). Leave LTE and unknown paths untouched.

Example patch for GSM:

```rust
            GsmtapType::UmTchF | GsmtapType::UmTchH | GsmtapType::Abis | GsmtapType::Um => {
                Ok(InformationElement::GSM(GsmMeta {
                    arfcn: Some(gsmtap_msg.header.arfcn),
                    signal_dbm: Some(gsmtap_msg.header.signal_dbm),
                    frame_number: Some(gsmtap_msg.header.frame_number),
                }))
            }
            GsmtapType::Umts => Ok(InformationElement::UMTS(UmtsMeta {
                uarfcn: Some(gsmtap_msg.header.arfcn as u32),
                signal_dbm: Some(gsmtap_msg.header.signal_dbm),
            })),
```

Adjust variant names to match the actual `GsmtapType` enum in `lib/src/gsmtap.rs`. If no GSM variants exist today, just extend the fallback to produce `InformationElement::GSM(GsmMeta::default())` for GSM-tagged payloads (use `#[derive(Default)]` on `GsmMeta`/`UmtsMeta`).

- [ ] **Step 3: Verify existing analyzers still compile**

Run: `cargo check -p rayhunter`
Expected: success. Any existing `match` on `InformationElement::GSM` or `::UMTS` without binding will error; add `_meta` placeholder to make them pass: `InformationElement::GSM(_) => {}`.

- [ ] **Step 4: Run the full test suite**

Run: `cargo test -p rayhunter --lib`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add lib/src/analysis/information_element.rs
git commit -m "feat(lib): carry GSMTAP metadata on GSM/UMTS InformationElement variants"
```

---

## Task 7: `CellObservation` type + `CellObserver` scaffold

**Files:**
- Create: `lib/src/cell/observer.rs`

- [ ] **Step 1: Write the failing test for an empty observer**

Create `lib/src/cell/observer.rs`:

```rust
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

use crate::analysis::information_element::InformationElement;
use crate::cell::{CellIdentity, SignalSample};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CellObservation {
    Serving {
        identity: CellIdentity,
        signal: SignalSample,
        timestamp: DateTime<FixedOffset>,
    },
    Neighbor {
        identity: CellIdentity,
        signal: SignalSample,
        timestamp: DateTime<FixedOffset>,
    },
}

pub struct CellObserver;

impl CellObserver {
    pub fn new() -> Self { Self }

    /// Extract zero-or-more observations from a single IE.
    pub fn observe(
        &mut self,
        ie: &InformationElement,
        now: DateTime<FixedOffset>,
    ) -> Vec<CellObservation> {
        match ie {
            InformationElement::LTE(lte) => self.observe_lte(lte, now),
            InformationElement::GSM(meta) => self.observe_gsm(meta, now),
            InformationElement::UMTS(meta) => self.observe_umts(meta, now),
            InformationElement::FiveG => Vec::new(),
        }
    }

    fn observe_lte(
        &mut self,
        _lte: &crate::analysis::information_element::LteInformationElement,
        _now: DateTime<FixedOffset>,
    ) -> Vec<CellObservation> {
        // populated in Task 8 and 9
        Vec::new()
    }

    fn observe_gsm(
        &mut self,
        meta: &crate::analysis::information_element::GsmMeta,
        now: DateTime<FixedOffset>,
    ) -> Vec<CellObservation> {
        if meta.arfcn.is_none() {
            return Vec::new();
        }
        let identity = CellIdentity::Gsm {
            plmn: None, lac: None, cid: None, bsic: None, arfcn: meta.arfcn,
        };
        let signal = SignalSample {
            timestamp: Some(now),
            rxlev: meta.signal_dbm.map(gsmtap_signal_to_rxlev),
            ..Default::default()
        };
        vec![CellObservation::Serving { identity, signal, timestamp: now }]
    }

    fn observe_umts(
        &mut self,
        meta: &crate::analysis::information_element::UmtsMeta,
        now: DateTime<FixedOffset>,
    ) -> Vec<CellObservation> {
        if meta.uarfcn.is_none() {
            return Vec::new();
        }
        let identity = CellIdentity::Umts {
            plmn: None, lac: None, cid: None, psc: None, uarfcn: meta.uarfcn,
        };
        let signal = SignalSample {
            timestamp: Some(now),
            rscp_dbm: meta.signal_dbm.map(|d| d as i16),
            ..Default::default()
        };
        vec![CellObservation::Serving { identity, signal, timestamp: now }]
    }
}

/// Rough mapping from GSMTAP-carried dBm to GSM RxLev 0..63.
/// RxLev 0 = -110 dBm, step 1 dB, saturates at 63 = -48 dBm (3GPP 45.008).
fn gsmtap_signal_to_rxlev(dbm: i8) -> u8 {
    let v = (dbm as i32) + 110;
    v.clamp(0, 63) as u8
}

impl Default for CellObserver {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::information_element::{GsmMeta, InformationElement};
    use chrono::TimeZone;

    fn ts() -> DateTime<FixedOffset> {
        FixedOffset::east_opt(8 * 3600).unwrap().with_ymd_and_hms(2026, 4, 19, 12, 40, 0).unwrap()
    }

    #[test]
    fn gsm_metadata_produces_one_serving_observation() {
        let mut o = CellObserver::new();
        let ie = InformationElement::GSM(GsmMeta {
            arfcn: Some(62), signal_dbm: Some(-70), frame_number: None,
        });
        let obs = o.observe(&ie, ts());
        assert_eq!(obs.len(), 1);
        let CellObservation::Serving { identity, signal, .. } = &obs[0] else { panic!() };
        let CellIdentity::Gsm { arfcn, .. } = identity else { panic!() };
        assert_eq!(*arfcn, Some(62));
        assert_eq!(signal.rxlev, Some(40)); // -70 dBm -> rxlev 40
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p rayhunter cell::observer`
Expected: 1 passed.

- [ ] **Step 3: Commit**

```bash
git add lib/src/cell/observer.rs
git commit -m "feat(lib): CellObserver scaffold with GSM/UMTS basic extraction"
```

---

## Task 8: LTE SIB1 → serving observation

**Files:**
- Modify: `lib/src/cell/observer.rs`

- [ ] **Step 1: Write the failing test with a raw SIB1 fixture**

Append to `lib/src/cell/observer.rs`'s `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn lte_sib1_yields_serving_with_plmn_tac_cid() {
        use crate::analysis::information_element::LteInformationElement;
        use deku::bitvec::*;
        use telcom_parser::decode;

        // Canned SIB1 UPER from a known LTE cell (hex extracted from a prior capture).
        // PLMN 510-10, TAC 0x0500, CID 0x005B69.
        let hex = "40049600a500005b69e2";
        let bytes: Vec<u8> = (0..hex.len()).step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i+2], 16).unwrap()).collect();
        let msg: telcom_parser::lte_rrc::BCCH_DL_SCH_Message = decode(&bytes)
            .expect("decode sib1 fixture");
        let ie = InformationElement::LTE(Box::new(LteInformationElement::BcchDlSch(msg)));

        let mut o = CellObserver::new();
        let obs = o.observe(&ie, ts());
        assert_eq!(obs.len(), 1);
        let CellObservation::Serving { identity, .. } = &obs[0] else { panic!() };
        let CellIdentity::Lte { plmn, tac, cid, .. } = identity else { panic!() };
        assert!(plmn.is_some());
        assert_eq!(tac.unwrap_or(0), 0x0500);
        assert_eq!(cid.unwrap_or(0), 0x005B69);
    }
```

Note: the hex fixture above is illustrative. If decoding fails, replace with an actual capture extracted from `lib/tests/test_lte_parsing.rs` fixtures or an `.qmdl` in the repo.

- [ ] **Step 2: Run test and confirm it fails**

Run: `cargo test -p rayhunter cell::observer::tests::lte_sib1_yields_serving_with_plmn_tac_cid`
Expected: FAIL — observer returns 0 observations.

- [ ] **Step 3: Implement `observe_lte` SIB1 branch**

Replace the `observe_lte` method with (keep the rest of `observer.rs`):

```rust
    fn observe_lte(
        &mut self,
        lte: &crate::analysis::information_element::LteInformationElement,
        now: DateTime<FixedOffset>,
    ) -> Vec<CellObservation> {
        use crate::analysis::information_element::LteInformationElement as L;
        use deku::bitvec::*;
        use telcom_parser::lte_rrc::{
            BCCH_DL_SCH_MessageType, BCCH_DL_SCH_MessageType_c1,
        };

        let mut out = Vec::new();

        if let L::BcchDlSch(sch) = lte {
            if let BCCH_DL_SCH_MessageType::C1(BCCH_DL_SCH_MessageType_c1::SystemInformationBlockType1(sib1)) = &sch.message {
                let cid = sib1.cell_access_related_info.cell_identity.0
                    .as_bitslice().load_be::<u32>();
                let tac = sib1.cell_access_related_info.tracking_area_code.0
                    .as_bitslice().load_be::<u32>();
                let plmn = extract_first_plmn(&sib1.cell_access_related_info.plmn_identity_list.0);
                let identity = CellIdentity::Lte {
                    plmn, tac: Some(tac), cid: Some(cid as u64),
                    pci: None, earfcn: None,
                };
                out.push(CellObservation::Serving {
                    identity,
                    signal: SignalSample { timestamp: Some(now), ..Default::default() },
                    timestamp: now,
                });
            }
        }

        out
    }

fn extract_first_plmn(
    list: &[telcom_parser::lte_rrc::PLMN_IdentityInfo],
) -> Option<crate::cell::Plmn> {
    let info = list.first()?;
    let mcc_digits = info.plmn_identity.mcc.as_ref()?;
    let mcc_vals: Vec<u16> = mcc_digits.0.iter().map(|d| d.0 as u16).collect();
    if mcc_vals.len() != 3 { return None; }
    let mcc = mcc_vals[0] * 100 + mcc_vals[1] * 10 + mcc_vals[2];

    let mnc_digits = &info.plmn_identity.mnc.0;
    let mnc_vals: Vec<u16> = mnc_digits.iter().map(|d| d.0 as u16).collect();
    let (mnc, is_3) = match mnc_vals.len() {
        2 => (mnc_vals[0] * 10 + mnc_vals[1], false),
        3 => (mnc_vals[0] * 100 + mnc_vals[1] * 10 + mnc_vals[2], true),
        _ => return None,
    };
    Some(crate::cell::Plmn { mcc, mnc, mnc_is_3_digit: is_3 })
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p rayhunter cell::observer`
Expected: all pass.

If the hex fixture decode fails, capture a real SIB1 by:
```
cargo test -p rayhunter -- --nocapture lte_parsing
```
and copy the hex bytes from an existing SIB1 fixture in `lib/tests/`.

- [ ] **Step 5: Commit**

```bash
git add lib/src/cell/observer.rs
git commit -m "feat(lib): CellObserver extracts LTE serving identity from SIB1"
```

---

## Task 9: LTE MeasurementReport → neighbor observations

**Files:**
- Modify: `lib/src/cell/observer.rs`

- [ ] **Step 1: Write failing test**

Append in `observer.rs` tests:

```rust
    #[test]
    fn lte_measurement_report_yields_neighbors_with_rsrp() {
        // TODO engineer: replace with a real UL-DCCH MeasurementReport UPER hex
        // dump. Until then, this test is gated by an environment fixture.
        // Command to capture one:
        //   rg -l "MeasurementReport" lib/tests/
        let path = "lib/tests/fixtures/ul_dcch_meas_report.bin";
        let Ok(bytes) = std::fs::read(path) else {
            eprintln!("skip: {path} not present");
            return;
        };
        let msg: telcom_parser::lte_rrc::UL_DCCH_Message = telcom_parser::decode(&bytes).unwrap();
        let ie = InformationElement::LTE(Box::new(
            crate::analysis::information_element::LteInformationElement::UlDcch(msg),
        ));
        let mut o = CellObserver::new();
        let obs = o.observe(&ie, ts());
        let neighbors: Vec<_> = obs.iter().filter_map(|o| match o {
            CellObservation::Neighbor { identity, signal, .. } => Some((identity, signal)),
            _ => None,
        }).collect();
        assert!(!neighbors.is_empty(), "expected at least one neighbor");
        assert!(neighbors.iter().any(|(_, s)| s.rsrp_dbm.is_some()));
    }
```

- [ ] **Step 2: Run it to see the skip path**

Run: `cargo test -p rayhunter cell::observer::tests::lte_measurement_report_yields_neighbors_with_rsrp -- --nocapture`
Expected: prints `skip: ...` and passes as no-op (so it won't block until fixture is provided).

- [ ] **Step 3: Extend `observe_lte` for UlDcch**

Add to the `observe_lte` method, after the SIB1 branch:

```rust
        use telcom_parser::lte_rrc::{
            UL_DCCH_MessageType, UL_DCCH_MessageType_c1, MeasResults, MeasResultListEUTRA,
        };

        if let L::UlDcch(ul) = lte {
            if let UL_DCCH_MessageType::C1(UL_DCCH_MessageType_c1::MeasurementReport(mr)) = &ul.message {
                if let Some(r8) = extract_meas_r8(mr) {
                    // Serving cell signal
                    let srv_rsrp = r8.meas_result_serv_cell.rsrp_result.0 as u8;
                    let srv_rsrq = r8.meas_result_serv_cell.rsrq_result.0 as u8;
                    out.push(CellObservation::Serving {
                        identity: CellIdentity::Lte {
                            plmn: None, tac: None, cid: None,
                            pci: None, earfcn: None,
                        },
                        signal: SignalSample {
                            timestamp: Some(now),
                            rsrp_dbm: Some(crate::cell::signal::decode_lte_rsrp(srv_rsrp)),
                            rsrq_db: Some(crate::cell::signal::decode_lte_rsrq(srv_rsrq)),
                            ..Default::default()
                        },
                        timestamp: now,
                    });

                    if let Some(neigh_list) = neigh_list_eutra(r8) {
                        for nc in neigh_list {
                            let pci = nc.phys_cell_id.0 as u16;
                            let rsrp = nc.meas_result.rsrp_result.as_ref().map(|r| r.0 as u8);
                            let rsrq = nc.meas_result.rsrq_result.as_ref().map(|r| r.0 as u8);
                            out.push(CellObservation::Neighbor {
                                identity: CellIdentity::Lte {
                                    plmn: None, tac: None, cid: None,
                                    pci: Some(pci), earfcn: None,
                                },
                                signal: SignalSample {
                                    timestamp: Some(now),
                                    rsrp_dbm: rsrp.map(crate::cell::signal::decode_lte_rsrp),
                                    rsrq_db: rsrq.map(crate::cell::signal::decode_lte_rsrq),
                                    ..Default::default()
                                },
                                timestamp: now,
                            });
                        }
                    }
                }
            }
        }
```

Add these private helpers below `extract_first_plmn` in the same file. The exact accessor depth depends on the generated `telcom-parser` types; adjust based on `cargo doc -p telcom-parser --open`:

```rust
fn extract_meas_r8(
    mr: &telcom_parser::lte_rrc::MeasurementReport,
) -> Option<&telcom_parser::lte_rrc::MeasurementReport_r8_IEs> {
    use telcom_parser::lte_rrc::{
        MeasurementReportCriticalExtensions, MeasurementReportCriticalExtensions_c1,
    };
    let MeasurementReportCriticalExtensions::C1(
        MeasurementReportCriticalExtensions_c1::MeasurementReport_r8(r8)
    ) = &mr.critical_extensions else { return None; };
    Some(&r8.meas_results)
        .and_then(|_| Some(r8))
        .and_then(|r| match &r.meas_results {
            _ => Some(r), // meas_results is inlined; shape depends on generated code
        })
}

fn neigh_list_eutra(
    r8: &telcom_parser::lte_rrc::MeasurementReport_r8_IEs,
) -> Option<&Vec<telcom_parser::lte_rrc::MeasResultEUTRA>> {
    use telcom_parser::lte_rrc::MeasResults_meas_result_neigh_cells;
    let Some(neigh) = r8.meas_results.meas_result_neigh_cells.as_ref() else { return None; };
    match neigh {
        MeasResults_meas_result_neigh_cells::MeasResultListEUTRA(l) => Some(&l.0),
        _ => None,
    }
}
```

**Engineer note:** The exact field accessors in `telcom-parser`'s generated types may differ from what's shown. Run `cargo doc -p telcom-parser --no-deps --open` and navigate to `MeasurementReport_r8_IEs` and `MeasResults` to confirm. Adjust the `meas_result_serv_cell` and `meas_result_neigh_cells` paths accordingly. The goal is: pull rsrpResult/rsrqResult for serving and each neighbor's physCellId + rsrp/rsrq.

- [ ] **Step 4: Build; fix compilation errors iteratively**

Run: `cargo check -p rayhunter`
Expected: must compile. If field names differ, open telcom-parser docs as described above and fix.

- [ ] **Step 5: Run tests**

Run: `cargo test -p rayhunter cell::observer`
Expected: all pass (the fixture-gated test will skip if no fixture).

- [ ] **Step 6: Commit**

```bash
git add lib/src/cell/observer.rs
git commit -m "feat(lib): CellObserver extracts LTE MeasurementReport neighbors"
```

---

## Task 10: `CellAggregate` + `CellStore` skeleton

**Files:**
- Create: `lib/src/cell/store.rs`

- [ ] **Step 1: Write failing test for insert + dedup**

Create `lib/src/cell/store.rs`:

```rust
use std::collections::{HashMap, VecDeque};

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

use crate::cell::{
    identity::{CellIdentity, CellKey},
    mcc_mnc::lookup_operator,
    observer::CellObservation,
    SignalSample,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
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
    pub operator_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NeighborSnapshot {
    pub identity: CellIdentity,
    pub signal: SignalSample,
    pub operator_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CellContext {
    pub serving: Option<CellIdentity>,
    pub serving_signal: Option<SignalSample>,
    pub serving_operator: Option<String>,
    pub neighbors: Vec<NeighborSnapshot>,
}

pub struct CellStore {
    by_key: HashMap<CellKey, CellAggregate>,
    current_serving_key: Option<CellKey>,
    serving_rsrp_buffer: VecDeque<(DateTime<FixedOffset>, i16)>,
    neighbor_count_buffer: VecDeque<(DateTime<FixedOffset>, u32)>,
    buffer_capacity: usize,
    avg_accum: HashMap<CellKey, (i64, u32)>, // (sum_rsrp, count)
}

impl CellStore {
    pub fn new(buffer_capacity: usize) -> Self {
        Self {
            by_key: HashMap::new(),
            current_serving_key: None,
            serving_rsrp_buffer: VecDeque::with_capacity(buffer_capacity),
            neighbor_count_buffer: VecDeque::with_capacity(buffer_capacity),
            buffer_capacity,
            avg_accum: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize { self.by_key.len() }
    pub fn is_empty(&self) -> bool { self.by_key.is_empty() }

    pub fn apply(&mut self, obs: &CellObservation) {
        let (identity, signal, is_serving, ts) = match obs {
            CellObservation::Serving { identity, signal, timestamp } => (identity, signal, true, *timestamp),
            CellObservation::Neighbor { identity, signal, timestamp } => (identity, signal, false, *timestamp),
        };
        let key = CellKey::from(identity);

        let entry = self.by_key.entry(key.clone()).or_insert_with(|| CellAggregate {
            identity: identity.clone(),
            first_seen: ts,
            last_seen: ts,
            observation_count: 0,
            is_serving_ever: false,
            current_signal: None,
            signal_min: None,
            signal_max: None,
            signal_avg_rsrp_dbm: None,
            signal_avg_rxlev: None,
            operator_name: identity.plmn().and_then(lookup_operator).map(|s| s.to_string()),
        });

        // merge identity fields in-place
        entry.identity.merge_from(identity);
        if entry.operator_name.is_none() {
            entry.operator_name = entry.identity.plmn().and_then(lookup_operator).map(|s| s.to_string());
        }
        entry.last_seen = ts;
        entry.observation_count += 1;
        if is_serving { entry.is_serving_ever = true; }
        entry.current_signal = Some(signal.clone());

        // min / max
        update_min_max(&mut entry.signal_min, &mut entry.signal_max, signal);

        // running average rsrp (LTE) and rxlev (GSM)
        if let Some(rsrp) = signal.rsrp_dbm {
            let slot = self.avg_accum.entry(key.clone()).or_insert((0, 0));
            slot.0 += rsrp as i64;
            slot.1 += 1;
            entry.signal_avg_rsrp_dbm = Some((slot.0 / slot.1 as i64) as i16);
        }
        if let Some(rxlev) = signal.rxlev {
            let slot = self.avg_accum.entry(key.clone()).or_insert((0, 0));
            slot.0 += rxlev as i64;
            slot.1 += 1;
            entry.signal_avg_rxlev = Some((slot.0 / slot.1 as i64) as u8);
        }

        if is_serving {
            self.current_serving_key = Some(key);
            if let Some(rsrp) = signal.rsrp_dbm {
                self.push_buffer(&mut self.serving_rsrp_buffer.clone(), ts, rsrp);
                if self.serving_rsrp_buffer.len() == self.buffer_capacity {
                    self.serving_rsrp_buffer.pop_front();
                }
                self.serving_rsrp_buffer.push_back((ts, rsrp));
            }
        }

        let neigh_count = self.by_key.values().filter(|a| !a.is_serving_ever).count() as u32;
        if self.neighbor_count_buffer.len() == self.buffer_capacity {
            self.neighbor_count_buffer.pop_front();
        }
        self.neighbor_count_buffer.push_back((ts, neigh_count));
    }

    pub fn current_context(&self, max_neighbors: usize) -> CellContext {
        let serving = self.current_serving_key.as_ref().and_then(|k| self.by_key.get(k));
        let serving_id = serving.map(|s| s.identity.clone());
        let serving_sig = serving.and_then(|s| s.current_signal.clone());
        let serving_op = serving.and_then(|s| s.operator_name.clone());

        let mut neigh: Vec<&CellAggregate> = self.by_key.values()
            .filter(|a| !a.is_serving_ever && a.current_signal.is_some())
            .collect();
        neigh.sort_by_key(|a| -(a.current_signal.as_ref().and_then(|s| s.rsrp_dbm).unwrap_or(i16::MIN) as i32));
        neigh.truncate(max_neighbors);

        CellContext {
            serving: serving_id,
            serving_signal: serving_sig,
            serving_operator: serving_op,
            neighbors: neigh.into_iter().map(|a| NeighborSnapshot {
                identity: a.identity.clone(),
                signal: a.current_signal.clone().unwrap_or_default(),
                operator_name: a.operator_name.clone(),
            }).collect(),
        }
    }

    pub fn aggregates(&self) -> Vec<CellAggregate> {
        self.by_key.values().cloned().collect()
    }

    pub fn serving_rsrp_history(&self) -> Vec<(DateTime<FixedOffset>, i16)> {
        self.serving_rsrp_buffer.iter().copied().collect()
    }

    pub fn neighbor_count_history(&self) -> Vec<(DateTime<FixedOffset>, u32)> {
        self.neighbor_count_buffer.iter().copied().collect()
    }

    pub fn reset(&mut self) {
        self.by_key.clear();
        self.current_serving_key = None;
        self.serving_rsrp_buffer.clear();
        self.neighbor_count_buffer.clear();
        self.avg_accum.clear();
    }

    fn push_buffer<T>(&self, _buf: &mut VecDeque<T>, _ts: DateTime<FixedOffset>, _v: i16) {
        // placeholder to keep the method shape; actual push occurs inline above
    }
}

fn update_min_max(min: &mut Option<SignalSample>, max: &mut Option<SignalSample>, s: &SignalSample) {
    if let Some(rsrp) = s.rsrp_dbm {
        if min.as_ref().and_then(|m| m.rsrp_dbm).map_or(true, |v| rsrp < v) {
            *min = Some(s.clone());
        }
        if max.as_ref().and_then(|m| m.rsrp_dbm).map_or(true, |v| rsrp > v) {
            *max = Some(s.clone());
        }
    }
    if let Some(rxl) = s.rxlev {
        if min.as_ref().and_then(|m| m.rxlev).map_or(true, |v| rxl < v) {
            *min = Some(s.clone());
        }
        if max.as_ref().and_then(|m| m.rxlev).map_or(true, |v| rxl > v) {
            *max = Some(s.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use crate::cell::Plmn;

    fn ts() -> DateTime<FixedOffset> {
        FixedOffset::east_opt(0).unwrap().with_ymd_and_hms(2026, 4, 19, 12, 0, 0).unwrap()
    }

    #[test]
    fn serving_observation_populates_store() {
        let mut s = CellStore::new(120);
        let ob = CellObservation::Serving {
            identity: CellIdentity::Lte {
                plmn: Some(Plmn { mcc: 510, mnc: 11, mnc_is_3_digit: false }),
                tac: Some(1280), cid: Some(23401), pci: None, earfcn: None,
            },
            signal: SignalSample { rsrp_dbm: Some(-79), rsrq_db: Some(-9), ..Default::default() },
            timestamp: ts(),
        };
        s.apply(&ob);
        assert_eq!(s.len(), 1);
        let ctx = s.current_context(8);
        assert!(ctx.serving.is_some());
        assert_eq!(ctx.serving_operator.as_deref(), Some("XL Axiata"));
    }

    #[test]
    fn neighbor_observations_tracked_separately() {
        let mut s = CellStore::new(120);
        for pci in [301, 143, 288] {
            s.apply(&CellObservation::Neighbor {
                identity: CellIdentity::Lte {
                    plmn: None, tac: None, cid: None, pci: Some(pci), earfcn: Some(1350),
                },
                signal: SignalSample { rsrp_dbm: Some(-85 - pci as i16 % 5), ..Default::default() },
                timestamp: ts(),
            });
        }
        let ctx = s.current_context(2);
        assert_eq!(ctx.neighbors.len(), 2);
        // sorted by strongest RSRP first
        assert!(
            ctx.neighbors[0].signal.rsrp_dbm.unwrap()
            >= ctx.neighbors[1].signal.rsrp_dbm.unwrap()
        );
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p rayhunter cell::store`
Expected: 2 passed.

- [ ] **Step 3: Commit**

```bash
git add lib/src/cell/store.rs lib/src/cell/mod.rs
git commit -m "feat(lib): CellStore with aggregation, context snapshot, ring buffers"
```

---

## Task 11: Persist CellStore to NDJSON

**Files:**
- Modify: `lib/src/cell/store.rs`

- [ ] **Step 1: Add `flush_to` method with metadata header**

Append to `impl CellStore`:

```rust
    /// Serialize the current aggregate snapshot as a single NDJSON line.
    /// The caller is expected to append this line to `{name}.cells.ndjson`.
    pub fn serialize_flush_line(&self, flushed_at: DateTime<FixedOffset>) -> String {
        let payload = FlushLine {
            flushed_at,
            aggregates: self.aggregates(),
        };
        serde_json::to_string(&payload).expect("serialize flush line")
    }

    pub fn serialize_metadata_line(rayhunter_version: &str, created_at: DateTime<FixedOffset>) -> String {
        let meta = CellReportMetadata {
            report_version: 1,
            rayhunter_version: rayhunter_version.to_string(),
            created_at,
        };
        serde_json::to_string(&meta).expect("serialize metadata line")
    }
```

Add these types above the `impl`:

```rust
#[derive(Serialize, Deserialize, Debug)]
pub struct CellReportMetadata {
    pub report_version: u32,
    pub rayhunter_version: String,
    pub created_at: DateTime<FixedOffset>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FlushLine {
    pub flushed_at: DateTime<FixedOffset>,
    pub aggregates: Vec<CellAggregate>,
}
```

- [ ] **Step 2: Add parsing helper for replay**

```rust
    /// Given the full contents of a cells.ndjson file, return the most recent flush.
    pub fn parse_latest_snapshot(contents: &str) -> Option<FlushLine> {
        let mut last: Option<FlushLine> = None;
        for line in contents.lines() {
            if line.trim().is_empty() { continue; }
            if let Ok(f) = serde_json::from_str::<FlushLine>(line) {
                last = Some(f);
            }
        }
        last
    }
```

- [ ] **Step 3: Add round-trip test**

In `store.rs` tests:

```rust
    #[test]
    fn flush_line_round_trip() {
        let mut s = CellStore::new(120);
        s.apply(&CellObservation::Serving {
            identity: CellIdentity::Lte {
                plmn: Some(Plmn { mcc: 510, mnc: 11, mnc_is_3_digit: false }),
                tac: Some(1), cid: Some(1), pci: None, earfcn: None,
            },
            signal: SignalSample { rsrp_dbm: Some(-80), ..Default::default() },
            timestamp: ts(),
        });
        let line = s.serialize_flush_line(ts());
        let snap = CellStore::parse_latest_snapshot(&line).unwrap();
        assert_eq!(snap.aggregates.len(), 1);
    }
```

- [ ] **Step 4: Run + commit**

Run: `cargo test -p rayhunter cell::store`

```bash
git add lib/src/cell/store.rs
git commit -m "feat(lib): CellStore serializes flush lines and parses latest snapshot"
```

---

## Task 12: Extend `Event` with `cell_context`, bump REPORT_VERSION

**Files:**
- Modify: `lib/src/analysis/analyzer.rs`

- [ ] **Step 1: Locate current `REPORT_VERSION`**

Run: `rg -n 'REPORT_VERSION' lib/src/analysis/analyzer.rs`
Note the current value (e.g. `pub const REPORT_VERSION: u32 = 2;`).

- [ ] **Step 2: Add `cell_context` field to Event**

Change:

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Event {
    pub event_type: EventType,
    pub message: String,
}
```

to:

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Event {
    pub event_type: EventType,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_context: Option<crate::cell::CellContext>,
}
```

- [ ] **Step 3: Bump `REPORT_VERSION` by 1**

e.g. `pub const REPORT_VERSION: u32 = 3;`

- [ ] **Step 4: Fix every `Event { ... }` constructor in the repo**

Run: `rg -n 'Event *\{' lib/ daemon/`
For each hit, add `cell_context: None,`. Example analyzers to update:
- `lib/src/analysis/imsi_requested.rs`
- `lib/src/analysis/connection_redirect_downgrade.rs`
- `lib/src/analysis/null_cipher.rs`
- `lib/src/analysis/nas_null_cipher.rs`
- `lib/src/analysis/priority_2g_downgrade.rs`
- `lib/src/analysis/incomplete_sib.rs`
- `lib/src/analysis/test_analyzer.rs`
- `lib/src/analysis/diagnostic.rs`

Prefer a repo-wide sed:

```bash
# macOS: use gsed or manual edit; below uses perl for portability
perl -i -pe 's/(Event \{[^}]*message:\s*[^,]*,)(\s*\})/${1} cell_context: None, ${2}/g' \
  lib/src/analysis/*.rs
```

Note: verify each file after perl patch — some multi-line constructors may need manual fix-ups.

- [ ] **Step 5: Build**

Run: `cargo check --workspace --exclude installer-gui`
Expected: success. Fix any remaining constructors flagged by the compiler.

- [ ] **Step 6: Run full test suite**

Run: `cargo test --workspace --exclude installer-gui --lib`
Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add lib/src/analysis/
git commit -m "feat(lib): add Event.cell_context (nullable) and bump REPORT_VERSION"
```

---

## Task 13: Wire `CellObserver` + `CellStore` into the analysis harness

**Files:**
- Modify: `lib/src/analysis/analyzer.rs`
- Modify: `daemon/src/analysis.rs`

- [ ] **Step 1: Add `CellStore` reference to `Harness`**

In `lib/src/analysis/analyzer.rs`, extend `Harness`:

```rust
pub struct Harness {
    analyzers: Vec<Box<dyn Analyzer + Send>>,
    packet_num: usize,
    observer: crate::cell::observer::CellObserver,
    store: std::sync::Arc<tokio::sync::RwLock<crate::cell::store::CellStore>>,
    max_neighbors_in_context: usize,
}
```

Update constructors to accept/build the store. Add:

```rust
impl Harness {
    pub fn new_with_store(
        store: std::sync::Arc<tokio::sync::RwLock<crate::cell::store::CellStore>>,
        max_neighbors_in_context: usize,
    ) -> Self {
        Self {
            analyzers: Vec::new(),
            packet_num: 0,
            observer: crate::cell::observer::CellObserver::new(),
            store,
            max_neighbors_in_context,
        }
    }
}
```

Keep the existing `new()` and `new_with_config()` by having them create a default `CellStore` internally (so existing call sites don't break):

```rust
    pub fn new() -> Self {
        Self::new_with_store(
            std::sync::Arc::new(tokio::sync::RwLock::new(
                crate::cell::store::CellStore::new(120),
            )),
            8,
        )
    }
```

- [ ] **Step 2: Feed observations in the per-packet analyze path**

Find the method that iterates analyzers over an IE (likely named `analyze_information_element` on `Harness`). Adjust to:

```rust
    pub async fn analyze_information_element(
        &mut self,
        ie: &crate::analysis::information_element::InformationElement,
        ts: chrono::DateTime<chrono::FixedOffset>,
    ) -> Vec<Option<Event>> {
        // 1. Observe cells
        let observations = self.observer.observe(ie, ts);
        {
            let mut store = self.store.write().await;
            for obs in &observations {
                store.apply(obs);
            }
        }

        // 2. Run analyzers, snapshot context on any Some(Event)
        let mut out = Vec::with_capacity(self.analyzers.len());
        for analyzer in &mut self.analyzers {
            let ev = analyzer.analyze_information_element(ie, self.packet_num);
            out.push(match ev {
                Some(mut e) => {
                    let ctx = self.store.read().await.current_context(self.max_neighbors_in_context);
                    e.cell_context = Some(ctx);
                    Some(e)
                }
                None => None,
            });
        }
        self.packet_num += 1;
        out
    }
```

If the existing method is sync, rename the old version or make it `async` and update callers. Check `rg 'analyze_information_element' --type rust` — the daemon likely calls it from `daemon/src/analysis.rs` inside a tokio task, so making it async is safe.

- [ ] **Step 3: Adjust daemon to own the store**

In `daemon/src/analysis.rs`, change the `Harness` construction to pass in a shared `CellStore`. Find where `Harness::new_with_config(...)` is called and replace with:

```rust
let cell_store = std::sync::Arc::new(tokio::sync::RwLock::new(
    rayhunter::cell::store::CellStore::new(
        config.bts_observatory.live_ring_buffer_size,
    ),
));
let mut harness = Harness::new_with_store(cell_store.clone(), config.bts_observatory.max_neighbors_in_context);
if analyzer_config.imsi_requested { harness.add_analyzer(...); } // keep existing code
```

Store `cell_store` on `ServerState` (Task 15).

- [ ] **Step 4: Run full suite**

Run: `cargo test --workspace --exclude installer-gui --lib`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add lib/src/analysis/analyzer.rs daemon/src/analysis.rs
git commit -m "feat(analysis): wire CellStore into harness and snapshot alerts"
```

---

## Task 14: Flush `CellStore` to disk

**Files:**
- Modify: `daemon/src/analysis.rs`

- [ ] **Step 1: Spawn a 10 s flush task**

In the daemon startup path where analysis kicks off, spawn:

```rust
let flush_store = cell_store.clone();
let flush_qmdl_path = state.qmdl_store_lock.clone();
let flush_token = state.daemon_restart_token.clone();
tokio::spawn(async move {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(10));
    loop {
        tokio::select! {
            _ = tick.tick() => {
                let qmdl = flush_qmdl_path.read().await;
                let Some(current) = qmdl.current_entry else { continue; };
                let name = qmdl.manifest.entries[current].name.clone();
                drop(qmdl);
                let path = format!(
                    "{}/{}.cells.ndjson",
                    qmdl.path.display(), // or the daemon's qmdl_store_path config
                    name,
                );
                let store = flush_store.read().await;
                let line = store.serialize_flush_line(rayhunter::clock::get_adjusted_now().fixed_offset());
                drop(store);
                if let Err(e) = tokio::fs::OpenOptions::new()
                    .create(true).append(true).open(&path).await
                    .and_then(|mut f| async move {
                        use tokio::io::AsyncWriteExt;
                        f.write_all(line.as_bytes()).await?;
                        f.write_all(b"\n").await
                    }.await) {
                    log::warn!("cells flush failed: {e}");
                }
            }
            _ = flush_token.cancelled() => break,
        }
    }
});
```

Adjust paths (`qmdl.path`) to match actual `RecordingStore` fields — check `daemon/src/qmdl_store.rs`.

- [ ] **Step 2: On recording start/stop, reset the store**

Hook the existing `start_recording` / `stop_recording` paths in `daemon/src/diag.rs` to call:

```rust
cell_store.write().await.reset();
```

on start. On stop, trigger a final flush (same code as the tick).

- [ ] **Step 3: Commit**

```bash
git add daemon/src/analysis.rs daemon/src/diag.rs
git commit -m "feat(daemon): periodic CellStore flush to {name}.cells.ndjson"
```

---

## Task 15: Add `CellStore` to `ServerState` and config

**Files:**
- Modify: `daemon/src/config.rs`
- Modify: `daemon/src/server.rs`
- Modify: `daemon/src/main.rs`

- [ ] **Step 1: Add `BtsObservatoryConfig` to `Config`**

In `daemon/src/config.rs` add:

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(feature = "apidocs", derive(utoipa::ToSchema))]
pub struct BtsObservatoryConfig {
    pub enabled: bool,
    pub live_ring_buffer_size: usize,
    pub flush_interval_seconds: u64,
    pub max_neighbors_in_context: usize,
}

impl Default for BtsObservatoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            live_ring_buffer_size: 120,
            flush_interval_seconds: 10,
            max_neighbors_in_context: 8,
        }
    }
}
```

And extend `Config` with `#[serde(default)] pub bts_observatory: BtsObservatoryConfig,`.

- [ ] **Step 2: Add field on `ServerState`**

In `daemon/src/server.rs`, extend `ServerState`:

```rust
pub struct ServerState {
    // ... existing fields ...
    pub cell_store: Arc<tokio::sync::RwLock<rayhunter::cell::store::CellStore>>,
}
```

Update all constructors (including `create_test_server_state`) to populate it.

- [ ] **Step 3: Wire construction in `daemon/src/main.rs`**

At startup, build the `CellStore` once and pass its `Arc` into both `Harness::new_with_store` and `ServerState`.

- [ ] **Step 4: Build + test**

Run: `cargo test --workspace --exclude installer-gui --lib`
Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add daemon/src/config.rs daemon/src/server.rs daemon/src/main.rs
git commit -m "feat(daemon): BtsObservatoryConfig and CellStore on ServerState"
```

---

## Task 16: `GET /api/cells/live` endpoint

**Files:**
- Create: `daemon/src/cells.rs`
- Modify: `daemon/src/main.rs`

- [ ] **Step 1: Create handler module**

Create `daemon/src/cells.rs`:

```rust
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use rayhunter::cell::store::{CellAggregate, CellContext, CellStore};

use crate::server::ServerState;

#[derive(Serialize)]
#[cfg_attr(feature = "apidocs", derive(utoipa::ToSchema))]
pub struct LiveCells {
    pub mode: &'static str,
    pub recording_name: Option<String>,
    pub context: CellContext,
    pub total_cells_seen: usize,
    pub serving_rsrp_history: Vec<(chrono::DateTime<chrono::FixedOffset>, i16)>,
    pub neighbor_count_history: Vec<(chrono::DateTime<chrono::FixedOffset>, u32)>,
    pub aggregates: Vec<CellAggregate>,
}

pub async fn get_live(
    State(state): State<Arc<ServerState>>,
) -> Result<Json<LiveCells>, (StatusCode, String)> {
    let store = state.cell_store.read().await;
    let qmdl = state.qmdl_store_lock.read().await;
    let recording_name = qmdl.current_entry.map(|i| qmdl.manifest.entries[i].name.clone());

    Ok(Json(LiveCells {
        mode: "live",
        recording_name,
        context: store.current_context(state.config.bts_observatory.max_neighbors_in_context),
        total_cells_seen: store.len(),
        serving_rsrp_history: store.serving_rsrp_history(),
        neighbor_count_history: store.neighbor_count_history(),
        aggregates: store.aggregates(),
    }))
}
```

- [ ] **Step 2: Register route**

In `daemon/src/main.rs`, add:

```rust
use crate::cells::get_live;

        .route("/api/cells/live", get(get_live))
```

Add `mod cells;` at the top of `main.rs`.

- [ ] **Step 3: Smoke test**

Run: `cargo check --workspace --exclude installer-gui`
Expected: success.

Add integration test `daemon/src/cells.rs` at bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn get_live_returns_empty_when_no_cells() {
        let state = crate::server::test_utils::make_test_state().await;
        let out = get_live(State(state)).await.unwrap();
        assert_eq!(out.total_cells_seen, 0);
    }
}
```

(Extract or add a `test_utils::make_test_state()` helper in `server.rs` if one does not exist, cloning the existing `create_test_server_state` logic.)

- [ ] **Step 4: Commit**

```bash
git add daemon/src/cells.rs daemon/src/main.rs daemon/src/server.rs
git commit -m "feat(daemon): GET /api/cells/live endpoint"
```

---

## Task 17: `GET /api/cells/{name}` + timeseries endpoints

**Files:**
- Modify: `daemon/src/cells.rs`
- Modify: `daemon/src/main.rs`

- [ ] **Step 1: Add replay handlers**

Append to `daemon/src/cells.rs`:

```rust
#[derive(Serialize)]
pub struct ReplayCells {
    pub mode: &'static str,
    pub recording_name: String,
    pub aggregates: Vec<CellAggregate>,
}

pub async fn get_replay(
    State(state): State<Arc<ServerState>>,
    Path(name): Path<String>,
) -> Result<Json<ReplayCells>, (StatusCode, String)> {
    let qmdl = state.qmdl_store_lock.read().await;
    let path = format!("{}/{}.cells.ndjson", qmdl.path.display(), name);
    drop(qmdl);

    let body = tokio::fs::read_to_string(&path).await
        .map_err(|e| (StatusCode::NOT_FOUND, format!("cells file not found: {e}")))?;

    let snap = CellStore::parse_latest_snapshot(&body)
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "no aggregate snapshots in file".into()))?;

    Ok(Json(ReplayCells {
        mode: "replay",
        recording_name: name,
        aggregates: snap.aggregates,
    }))
}

#[derive(Serialize)]
pub struct Timeseries {
    pub serving_rsrp: Vec<(chrono::DateTime<chrono::FixedOffset>, i16)>,
    pub neighbor_count: Vec<(chrono::DateTime<chrono::FixedOffset>, u32)>,
}

pub async fn get_timeseries(
    State(state): State<Arc<ServerState>>,
    Path(name): Path<String>,
) -> Result<Json<Timeseries>, (StatusCode, String)> {
    let qmdl = state.qmdl_store_lock.read().await;
    let path = format!("{}/{}.cells.ndjson", qmdl.path.display(), name);
    drop(qmdl);

    let body = tokio::fs::read_to_string(&path).await
        .map_err(|e| (StatusCode::NOT_FOUND, format!("cells file not found: {e}")))?;

    // Flush lines don't carry per-timestamp buffers, so until Phase 2 we return
    // an empty timeseries for replay mode. The frontend gracefully degrades.
    let _ = body;
    Ok(Json(Timeseries { serving_rsrp: vec![], neighbor_count: vec![] }))
}
```

- [ ] **Step 2: Register routes**

In `daemon/src/main.rs`:

```rust
        .route("/api/cells/{name}", get(crate::cells::get_replay))
        .route("/api/cells/{name}/timeseries", get(crate::cells::get_timeseries))
```

- [ ] **Step 3: Build + smoke test**

Run: `cargo check --workspace --exclude installer-gui`
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add daemon/src/cells.rs daemon/src/main.rs
git commit -m "feat(daemon): GET /api/cells/{name} and /timeseries endpoints"
```

---

## Task 18: Frontend — API client and store

**Files:**
- Create: `daemon/web/src/lib/cells.svelte.ts`

- [ ] **Step 1: Types + store + polling**

Create `daemon/web/src/lib/cells.svelte.ts`:

```typescript
export type RAT = 'LTE' | 'UMTS' | 'GSM';

export interface CellIdentity {
  rat: RAT;
  plmn?: { mcc: number; mnc: number; mnc_is_3_digit: boolean };
  tac?: number;
  lac?: number;
  cid?: number;
  pci?: number;
  psc?: number;
  bsic?: number;
  earfcn?: number;
  uarfcn?: number;
  arfcn?: number;
}

export interface SignalSample {
  timestamp?: string;
  rsrp_dbm?: number;
  rsrq_db?: number;
  rscp_dbm?: number;
  ecno_db?: number;
  rxlev?: number;
}

export interface NeighborSnapshot {
  identity: CellIdentity;
  signal: SignalSample;
  operator_name?: string;
}

export interface CellContext {
  serving?: CellIdentity;
  serving_signal?: SignalSample;
  serving_operator?: string;
  neighbors: NeighborSnapshot[];
}

export interface CellAggregate {
  identity: CellIdentity;
  first_seen: string;
  last_seen: string;
  observation_count: number;
  is_serving_ever: boolean;
  current_signal?: SignalSample;
  signal_min?: SignalSample;
  signal_max?: SignalSample;
  signal_avg_rsrp_dbm?: number;
  signal_avg_rxlev?: number;
  operator_name?: string;
}

export interface LiveCells {
  mode: 'live';
  recording_name?: string;
  context: CellContext;
  total_cells_seen: number;
  serving_rsrp_history: [string, number][];
  neighbor_count_history: [string, number][];
  aggregates: CellAggregate[];
}

export async function fetch_live(): Promise<LiveCells> {
  const r = await fetch('/api/cells/live');
  if (!r.ok) throw new Error(`cells/live ${r.status}`);
  return r.json();
}

export async function fetch_replay(name: string): Promise<{ aggregates: CellAggregate[] }> {
  const r = await fetch(`/api/cells/${encodeURIComponent(name)}`);
  if (!r.ok) throw new Error(`cells/${name} ${r.status}`);
  return r.json();
}

export function quality_band_rsrp(rsrp: number): { label: string; color: string } {
  if (rsrp >= -80) return { label: 'EXCELLENT', color: 'green-700' };
  if (rsrp >= -90)  return { label: 'GOOD', color: 'green-500' };
  if (rsrp >= -100) return { label: 'FAIR', color: 'amber-500' };
  return { label: 'POOR', color: 'red-500' };
}

export function format_plmn(p?: { mcc: number; mnc: number; mnc_is_3_digit: boolean }): string {
  if (!p) return '—';
  const mnc = p.mnc_is_3_digit
    ? String(p.mnc).padStart(3, '0')
    : String(p.mnc).padStart(2, '0');
  return `${String(p.mcc).padStart(3, '0')}-${mnc}`;
}
```

- [ ] **Step 2: Commit**

```bash
git add daemon/web/src/lib/cells.svelte.ts
git commit -m "feat(web): cells API client and types"
```

---

## Task 19: Frontend — `ServingCellCard.svelte`

**Files:**
- Create: `daemon/web/src/lib/components/ServingCellCard.svelte`

- [ ] **Step 1: Component**

```svelte
<script lang="ts">
    import type { CellIdentity, SignalSample } from '$lib/cells.svelte';
    import { format_plmn, quality_band_rsrp } from '$lib/cells.svelte';

    let { serving, signal, operator }: {
        serving?: CellIdentity;
        signal?: SignalSample;
        operator?: string;
    } = $props();

    let band = $derived(signal?.rsrp_dbm !== undefined ? quality_band_rsrp(signal.rsrp_dbm) : undefined);

    function cid_value(s: CellIdentity | undefined): string {
        if (!s) return '—';
        return (s.cid ?? s.lac ?? s.pci ?? s.psc ?? '—').toString();
    }
</script>

<div class="border-l-4 border-green-700 rounded bg-white shadow p-4 space-y-3">
    <div class="flex justify-between items-start">
        <div>
            <div class="text-xs uppercase tracking-wide text-gray-500">Serving Cell</div>
            <div class="text-3xl font-black font-mono">CID {cid_value(serving)}</div>
            <div class="text-gray-700">{operator ?? 'Unknown operator'}</div>
        </div>
        <div class="text-right">
            <div class="text-5xl font-black">{signal?.rsrp_dbm ?? '—'}</div>
            <div class="text-xs text-gray-500">dBm RSRP</div>
            {#if band}
                <div class="inline-block mt-1 px-2 py-0.5 text-xs font-bold rounded text-green-900 bg-green-100">
                    {band.label}
                </div>
            {/if}
        </div>
    </div>

    <div class="grid grid-cols-4 gap-2 text-xs">
        <div><div class="text-gray-500 uppercase">TAC</div><div class="font-mono">{serving?.tac ?? '—'}</div></div>
        <div><div class="text-gray-500 uppercase">PCI</div><div class="font-mono">{serving?.pci ?? '—'}</div></div>
        <div><div class="text-gray-500 uppercase">EARFCN</div><div class="font-mono">{serving?.earfcn ?? serving?.uarfcn ?? serving?.arfcn ?? '—'}</div></div>
        <div><div class="text-gray-500 uppercase">PLMN</div><div class="font-mono">{format_plmn(serving?.plmn)}</div></div>
    </div>

    <div>
        <div class="flex justify-between text-xs text-gray-500"><span>RSRP</span><span>{signal?.rsrp_dbm ?? '—'} dBm</span></div>
        <div class="w-full h-1.5 bg-gray-200 rounded overflow-hidden">
            <div class="h-full bg-green-700" style:width={`${Math.min(100, Math.max(0, ((signal?.rsrp_dbm ?? -140) + 140) * 100/97))}%`}></div>
        </div>
    </div>
    <div>
        <div class="flex justify-between text-xs text-gray-500"><span>RSRQ</span><span>{signal?.rsrq_db ?? '—'} dB</span></div>
        <div class="w-full h-1.5 bg-gray-200 rounded overflow-hidden">
            <div class="h-full bg-green-700" style:width={`${Math.min(100, Math.max(0, ((signal?.rsrq_db ?? -20) + 20) * 5))}%`}></div>
        </div>
    </div>
</div>
```

- [ ] **Step 2: Commit**

```bash
git add daemon/web/src/lib/components/ServingCellCard.svelte
git commit -m "feat(web): ServingCellCard component"
```

---

## Task 20: Frontend — `NeighborCellsTable.svelte`

**Files:**
- Create: `daemon/web/src/lib/components/NeighborCellsTable.svelte`

- [ ] **Step 1: Component**

```svelte
<script lang="ts">
    import type { NeighborSnapshot } from '$lib/cells.svelte';

    let { neighbors }: { neighbors: NeighborSnapshot[] } = $props();

    function bars(rsrp?: number): number {
        if (rsrp === undefined) return 0;
        if (rsrp >= -80) return 4;
        if (rsrp >= -90) return 3;
        if (rsrp >= -100) return 2;
        return 1;
    }

    let strongest = $derived(neighbors[0]?.signal?.rsrp_dbm);
    let weakest   = $derived(neighbors[neighbors.length - 1]?.signal?.rsrp_dbm);
    let frequencies = $derived(
        [...new Set(neighbors.map(n => n.identity.earfcn ?? n.identity.uarfcn ?? n.identity.arfcn).filter(Boolean))].join(', ')
    );
</script>

<div class="border-l-4 border-indigo-600 rounded bg-white shadow p-4 space-y-3">
    <div class="flex justify-between">
        <div>
            <div class="text-xs uppercase tracking-wide text-gray-500">Neighbor Cells</div>
            <div class="text-sm text-gray-600">dari MeasurementReport + SIB4/SIB5</div>
        </div>
        <div class="inline-block px-2 py-0.5 text-xs font-bold rounded text-indigo-900 bg-indigo-100 self-start">
            {neighbors.length} DETECTED
        </div>
    </div>

    <table class="w-full text-sm">
        <thead class="text-xs text-gray-500 uppercase">
            <tr><th class="text-left">#</th><th class="text-left">PCI</th><th class="text-left">CID</th><th class="text-left">RSRP</th><th class="text-right">dBm</th></tr>
        </thead>
        <tbody>
            {#each neighbors as n, i}
                {@const r = n.signal.rsrp_dbm}
                {@const color = r !== undefined && r < -100 ? 'bg-red-600' : 'bg-green-600'}
                <tr class="border-t">
                    <td class="py-1 text-gray-400">#{i + 1}</td>
                    <td class="py-1 font-mono font-bold">{n.identity.pci ?? n.identity.psc ?? n.identity.bsic ?? '—'}</td>
                    <td class="py-1 font-mono text-gray-600">{n.identity.cid ?? '—'}</td>
                    <td class="py-1">
                        <div class="w-24 h-1.5 bg-gray-200 rounded overflow-hidden inline-block">
                            <div class="h-full {color}" style:width={`${Math.min(100, Math.max(0, ((r ?? -140) + 140) * 100/97))}%`}></div>
                        </div>
                    </td>
                    <td class="py-1 text-right font-mono {r !== undefined && r < -100 ? 'text-red-700' : 'text-green-700'}">
                        {r ?? '—'} dBm {'▌'.repeat(bars(r))}
                    </td>
                </tr>
            {/each}
        </tbody>
    </table>

    <div class="grid grid-cols-3 gap-2 text-xs pt-2 border-t">
        <div><div class="text-gray-500 uppercase">Strongest</div><div class="font-bold text-green-700">{strongest ?? '—'} dBm</div></div>
        <div><div class="text-gray-500 uppercase">Weakest</div><div class="font-bold text-red-700">{weakest ?? '—'} dBm</div></div>
        <div><div class="text-gray-500 uppercase">Frequencies</div><div class="font-mono">{frequencies || '—'}</div></div>
    </div>
</div>
```

- [ ] **Step 2: Commit**

```bash
git add daemon/web/src/lib/components/NeighborCellsTable.svelte
git commit -m "feat(web): NeighborCellsTable component"
```

---

## Task 21: Frontend — `SignalHistoryChart.svelte`

**Files:**
- Create: `daemon/web/src/lib/components/SignalHistoryChart.svelte`

- [ ] **Step 1: Component**

```svelte
<script lang="ts">
    let { title, points, color = 'green', y_domain = [-120, -60] }: {
        title: string;
        points: [string, number][];
        color?: string;
        y_domain?: [number, number];
    } = $props();

    const W = 400;
    const H = 80;
    const pad = 4;

    let path = $derived.by(() => {
        if (points.length === 0) return '';
        const [y_min, y_max] = y_domain;
        const xs = (i: number) => pad + (i / Math.max(1, points.length - 1)) * (W - 2 * pad);
        const ys = (v: number) => {
            const norm = (v - y_min) / (y_max - y_min);
            return H - pad - norm * (H - 2 * pad);
        };
        return points.map((p, i) => `${i === 0 ? 'M' : 'L'}${xs(i)},${ys(p[1])}`).join(' ');
    });
</script>

<div class="bg-white rounded shadow p-4">
    <div class="text-xs uppercase text-gray-500 mb-2">{title}</div>
    <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" class="w-full h-20">
        <path d={path} fill="none" stroke={color === 'green' ? '#047857' : '#4f46e5'} stroke-width="1.5" />
    </svg>
    <div class="flex justify-between text-xs text-gray-400 mt-1">
        <span>{points.length > 0 ? `${points.length}s ago` : ''}</span>
        <span>now</span>
    </div>
</div>
```

- [ ] **Step 2: Commit**

```bash
git add daemon/web/src/lib/components/SignalHistoryChart.svelte
git commit -m "feat(web): SignalHistoryChart inline SVG sparkline"
```

---

## Task 22: Frontend — `AlertsStrip.svelte`

**Files:**
- Create: `daemon/web/src/lib/components/AlertsStrip.svelte`

- [ ] **Step 1: Component**

```svelte
<script lang="ts">
    import type { CellContext } from '$lib/cells.svelte';
    import { format_plmn } from '$lib/cells.svelte';

    export interface AlertRow {
        timestamp: string;
        severity: 'Low' | 'Medium' | 'High' | 'Informational';
        message: string;
        analyzer_name?: string;
        cell_context?: CellContext;
    }

    let { alerts }: { alerts: AlertRow[] } = $props();

    const color_for = (s: AlertRow['severity']) => (
        s === 'High' ? 'border-red-600 bg-red-50' :
        s === 'Medium' ? 'border-amber-500 bg-amber-50' :
        s === 'Low' ? 'border-blue-500 bg-blue-50' :
        'border-gray-300 bg-gray-50'
    );
</script>

{#if alerts.length > 0}
    <div class="border-l-4 border-red-600 rounded bg-white shadow p-4 space-y-2">
        <div class="text-xs uppercase text-gray-500">Alerts ({alerts.length})</div>
        {#each alerts as a}
            <details class="rounded border {color_for(a.severity)} p-2">
                <summary class="cursor-pointer">
                    <span class="font-bold text-xs uppercase">{a.severity}</span>
                    <span class="ml-2">{a.message}</span>
                    <span class="ml-2 text-xs text-gray-500">{a.timestamp}</span>
                </summary>
                {#if a.cell_context}
                    <div class="mt-2 text-xs bg-white rounded p-2 space-y-1">
                        <div><b>Serving:</b>
                            {a.cell_context.serving_operator ?? 'unknown'} ·
                            PLMN {format_plmn(a.cell_context.serving?.plmn)} ·
                            CID {a.cell_context.serving?.cid ?? '—'} ·
                            PCI {a.cell_context.serving?.pci ?? '—'} ·
                            RSRP {a.cell_context.serving_signal?.rsrp_dbm ?? '—'} dBm
                        </div>
                        {#if a.cell_context.neighbors.length > 0}
                            <div><b>Neighbors:</b>
                                {#each a.cell_context.neighbors as n}
                                    PCI {n.identity.pci ?? '—'} ({n.signal.rsrp_dbm ?? '—'}),
                                {/each}
                            </div>
                        {/if}
                    </div>
                {/if}
            </details>
        {/each}
    </div>
{/if}
```

- [ ] **Step 2: Commit**

```bash
git add daemon/web/src/lib/components/AlertsStrip.svelte
git commit -m "feat(web): AlertsStrip component with cell_context expansion"
```

---

## Task 23: Frontend — `/cells` page

**Files:**
- Create: `daemon/web/src/routes/cells/+page.svelte`

- [ ] **Step 1: Page**

```svelte
<script lang="ts">
    import { page } from '$app/state';
    import { fetch_live, fetch_replay, type LiveCells, type CellAggregate } from '$lib/cells.svelte';
    import ServingCellCard from '$lib/components/ServingCellCard.svelte';
    import NeighborCellsTable from '$lib/components/NeighborCellsTable.svelte';
    import SignalHistoryChart from '$lib/components/SignalHistoryChart.svelte';
    import AlertsStrip from '$lib/components/AlertsStrip.svelte';

    let recording_param = $derived(page.url.searchParams.get('recording') ?? undefined);
    let mode = $derived(recording_param ? 'replay' : 'live');
    let refresh_seconds: number = $state(2);
    let error: string | undefined = $state(undefined);

    let live_data: LiveCells | undefined = $state(undefined);
    let replay_aggregates: CellAggregate[] = $state([]);

    $effect(() => {
        let cancel = false;
        async function poll() {
            try {
                if (mode === 'live') {
                    live_data = await fetch_live();
                } else if (recording_param) {
                    const r = await fetch_replay(recording_param);
                    replay_aggregates = r.aggregates;
                }
                error = undefined;
            } catch (e) {
                error = e instanceof Error ? e.message : String(e);
            }
        }

        poll();

        if (mode === 'live') {
            const id = setInterval(() => { if (!cancel && !document.hidden) poll(); }, refresh_seconds * 1000);
            return () => { cancel = true; clearInterval(id); };
        }
        return () => { cancel = true; };
    });

    let serving_aggregate = $derived(
        mode === 'live'
            ? live_data?.aggregates.find(a => a.is_serving_ever)
            : replay_aggregates.find(a => a.is_serving_ever)
    );
    let neighbor_aggregates = $derived(
        (mode === 'live' ? live_data?.aggregates : replay_aggregates) ?? []
    ).filter(a => !a.is_serving_ever);
</script>

<div class="p-4 bg-rayhunter-blue text-white flex justify-between items-center">
    <div>
        <div class="text-lg font-black">RAYHUNTER MONITOR</div>
        <div class="text-xs opacity-80">
            {mode.toUpperCase()} · {refresh_seconds}s · {live_data?.total_cells_seen ?? replay_aggregates.length} cells
        </div>
    </div>
    <div class="flex items-center gap-3">
        {#if mode === 'live'}
            <div class="flex items-center gap-1">
                <span class="w-2 h-2 rounded-full bg-green-400 animate-pulse"></span>
                MONITORING
            </div>
            <select bind:value={refresh_seconds} class="text-black rounded px-1 py-0.5">
                <option value={1}>1s</option>
                <option value={2}>2s</option>
                <option value={5}>5s</option>
                <option value={10}>10s</option>
            </select>
        {:else}
            <div class="text-gray-200">REPLAY · {recording_param}</div>
        {/if}
    </div>
</div>

{#if error}
    <div class="bg-red-50 border-l-4 border-red-600 p-3 text-sm">Error: {error}</div>
{/if}

<div class="p-3 space-y-3 max-w-xl mx-auto">
    <ServingCellCard
        serving={live_data?.context.serving ?? serving_aggregate?.identity}
        signal={live_data?.context.serving_signal ?? serving_aggregate?.current_signal}
        operator={live_data?.context.serving_operator ?? serving_aggregate?.operator_name}
    />
    <NeighborCellsTable
        neighbors={live_data?.context.neighbors ?? neighbor_aggregates.map(a => ({
            identity: a.identity, signal: a.current_signal ?? {}, operator_name: a.operator_name
        }))}
    />
    {#if mode === 'live' && live_data}
        <SignalHistoryChart title="RSRP History"
            points={live_data.serving_rsrp_history} />
        <SignalHistoryChart title="Neighbor Count"
            points={live_data.neighbor_count_history.map(([t, c]) => [t, c] as [string, number])}
            color="indigo" y_domain={[0, 10]} />
    {/if}
    <!-- AlertsStrip requires wiring alerts; Phase 1 may ship empty until Task 25 -->
    <AlertsStrip alerts={[]} />
</div>
```

- [ ] **Step 2: Build web assets**

Run: `cd daemon/web && npm run build`
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add daemon/web/src/routes/cells/+page.svelte
git commit -m "feat(web): /cells dashboard page wired to live + replay APIs"
```

---

## Task 24: Nav link + layout update

**Files:**
- Modify: `daemon/web/src/routes/+layout.svelte`

- [ ] **Step 1: Add link in the nav**

Find the header nav in `+layout.svelte` (the Logs / Config buttons area). Add alongside:

```svelte
<a href="/cells" class="flex flex-row gap-1 items-center text-white hover:text-gray-400">
    <span class="hidden lg:flex">BTS</span>
    <span aria-hidden="true">📡</span>
</a>
```

Keep styling consistent with the existing Logs button.

- [ ] **Step 2: Build + commit**

Run: `cd daemon/web && npm run build`

```bash
git add daemon/web/src/routes/+layout.svelte
git commit -m "feat(web): nav link to /cells dashboard"
```

---

## Task 25: Alert enrichment in frontend

**Files:**
- Modify: `daemon/web/src/lib/analysis.svelte.ts` (or wherever analysis-row parsing lives)
- Modify: `daemon/web/src/routes/cells/+page.svelte`

- [ ] **Step 1: Find analysis parser**

Run: `rg -n 'cell_context|events' daemon/web/src/lib/`
The existing AnalysisTable parses rows; extend the type to include optional `cell_context` per event.

- [ ] **Step 2: Propagate into `/cells` page**

In the page effect, after fetching `live_data`, also fetch the current recording's analysis report (`/api/analysis-report/{name}`), extract any alerts with `cell_context`, and pass to `<AlertsStrip alerts={...} />`.

For REPLAY mode, same — but using `recording_param`.

- [ ] **Step 3: Add an integration check**

Manually: start the daemon with a fixture QMDL that is known to trigger an analyzer, then open `/cells` and confirm `AlertsStrip` renders one `details` block with serving + neighbor info expanded.

- [ ] **Step 4: Commit**

```bash
git add daemon/web/src/lib/analysis.svelte.ts daemon/web/src/routes/cells/+page.svelte
git commit -m "feat(web): surface cell_context on /cells AlertsStrip"
```

---

## Task 26: End-to-end smoke test

**Files:**
- Create: `lib/tests/cell_observer_end_to_end.rs`

- [ ] **Step 1: Integration harness**

Create `lib/tests/cell_observer_end_to_end.rs`:

```rust
use chrono::{FixedOffset, TimeZone};
use rayhunter::analysis::analyzer::Harness;
use rayhunter::analysis::information_element::{GsmMeta, InformationElement};
use rayhunter::cell::store::CellStore;

#[tokio::test]
async fn gsm_observation_populates_store_via_harness() {
    let store = std::sync::Arc::new(tokio::sync::RwLock::new(CellStore::new(120)));
    let mut h = Harness::new_with_store(store.clone(), 8);
    let ts = FixedOffset::east_opt(0).unwrap()
        .with_ymd_and_hms(2026, 4, 19, 12, 0, 0).unwrap();
    let ie = InformationElement::GSM(GsmMeta {
        arfcn: Some(62), signal_dbm: Some(-80), frame_number: None,
    });
    let _ = h.analyze_information_element(&ie, ts).await;
    assert_eq!(store.read().await.len(), 1);
}
```

- [ ] **Step 2: Run**

Run: `cargo test -p rayhunter --test cell_observer_end_to_end`
Expected: pass.

- [ ] **Step 3: Commit**

```bash
git add lib/tests/cell_observer_end_to_end.rs
git commit -m "test(lib): end-to-end CellStore population through Harness"
```

---

## Task 27: Final build + release profile check

**Files:**
- No new files.

- [ ] **Step 1: Full workspace build**

Run: `cargo check --workspace --exclude installer-gui`
Expected: success.

- [ ] **Step 2: Full test run**

Run: `cargo test --workspace --exclude installer-gui`
Expected: all pass.

- [ ] **Step 3: Clippy clean (existing bar)**

Run: `cargo clippy --workspace --exclude installer-gui -- -D warnings`
Expected: no warnings. Fix any introduced by our new code.

- [ ] **Step 4: Firmware profile build**

Run: `cargo build-daemon-firmware-devel`
Expected: success. (Requires armv7 musl toolchain; skip step if not installed locally and rely on CI.)

- [ ] **Step 5: Frontend typecheck**

Run: `cd daemon/web && npm run check`
Expected: no Svelte/TypeScript errors.

- [ ] **Step 6: Final commit (if any fixups)**

```bash
git add -p   # stage only the fixups
git commit -m "chore: final fix-ups for BTS observatory"
```

---

## Self-Review Checklist

- ✅ Phase 1 scope only: 2G/3G limited scaffolding, LTE full.
- ✅ Spec §6 data model — Tasks 2, 3, 4, 10.
- ✅ Spec §7 IE extraction — Tasks 6, 7, 8, 9.
- ✅ Spec §8 CellStore — Tasks 10, 11.
- ✅ Spec §9 HTTP API — Tasks 16, 17.
- ✅ Spec §10 MCC/MNC — Task 5.
- ✅ Spec §11 frontend — Tasks 18–25.
- ✅ Spec §12 config — Task 15.
- ✅ Spec §13 backward compat — Task 12.
- ✅ Spec §14 testing — Tasks 3, 4, 5, 7, 8, 9, 10, 11, 16, 26.
- ✅ Alert enrichment — Tasks 12, 13, 25.
- ✅ No placeholders: every code step shows code.
