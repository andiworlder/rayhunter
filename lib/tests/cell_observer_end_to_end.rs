//! End-to-end integration test: confirm a GSM IE feeds through the Harness
//! and populates the CellStore.
//!
//! Note: `Harness::analyze_information_element` is a private async method, so
//! this test exercises the same code path by driving the two public building
//! blocks that Harness uses internally: `CellObserver::observe` (to produce
//! `CellObservation` values) and `CellStore::apply` (to record them).  The
//! result is identical to what the Harness does on every packet.

use chrono::{FixedOffset, TimeZone};
use rayhunter::analysis::information_element::{GsmMeta, InformationElement};
use rayhunter::cell::observer::CellObserver;
use rayhunter::cell::store::CellStore;

#[test]
fn gsm_observation_populates_store_via_harness() {
    let mut store = CellStore::new(120);
    let mut observer = CellObserver::new();

    let ts = FixedOffset::east_opt(0)
        .unwrap()
        .with_ymd_and_hms(2026, 4, 19, 12, 0, 0)
        .unwrap();

    let ie = InformationElement::GSM(GsmMeta {
        arfcn: Some(62),
        signal_dbm: Some(-80),
        frame_number: None,
    });

    // This mirrors exactly what Harness::analyze_information_element does:
    //   let observations = self.observer.observe(ie, ts);
    //   for obs in &observations { store.apply(obs); }
    let observations = observer.observe(&ie, ts);
    for obs in &observations {
        store.apply(obs);
    }

    assert_eq!(store.len(), 1, "store should have one cell");

    let ctx = store.current_context(8);
    // GSM observation is marked serving; verify context reflects it
    assert!(ctx.serving.is_some(), "serving should be set");
}
