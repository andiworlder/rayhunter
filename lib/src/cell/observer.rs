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
    pub fn new() -> Self {
        Self
    }

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
            plmn: None,
            lac: None,
            cid: None,
            bsic: None,
            arfcn: meta.arfcn,
        };
        let signal = SignalSample {
            timestamp: Some(now),
            rxlev: meta.signal_dbm.map(gsmtap_signal_to_rxlev),
            ..Default::default()
        };
        vec![CellObservation::Serving {
            identity,
            signal,
            timestamp: now,
        }]
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
            plmn: None,
            lac: None,
            cid: None,
            psc: None,
            uarfcn: meta.uarfcn,
        };
        let signal = SignalSample {
            timestamp: Some(now),
            rscp_dbm: meta.signal_dbm.map(|d| d as i16),
            ..Default::default()
        };
        vec![CellObservation::Serving {
            identity,
            signal,
            timestamp: now,
        }]
    }
}

impl Default for CellObserver {
    fn default() -> Self {
        Self::new()
    }
}

/// Rough mapping from GSMTAP-carried dBm to GSM RxLev 0..63.
/// RxLev 0 = -110 dBm, step 1 dB, saturates at 63 = -48 dBm (3GPP 45.008).
fn gsmtap_signal_to_rxlev(dbm: i8) -> u8 {
    let v = (dbm as i32) + 110;
    v.clamp(0, 63) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::information_element::{GsmMeta, InformationElement};
    use chrono::TimeZone;

    fn ts() -> DateTime<FixedOffset> {
        FixedOffset::east_opt(8 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 4, 19, 12, 40, 0)
            .unwrap()
    }

    #[test]
    fn gsm_metadata_produces_one_serving_observation() {
        let mut o = CellObserver::new();
        let ie = InformationElement::GSM(GsmMeta {
            arfcn: Some(62),
            signal_dbm: Some(-70),
            frame_number: None,
        });
        let obs = o.observe(&ie, ts());
        assert_eq!(obs.len(), 1);
        let CellObservation::Serving { identity, signal, .. } = &obs[0] else {
            panic!()
        };
        let CellIdentity::Gsm { arfcn, .. } = identity else {
            panic!()
        };
        assert_eq!(*arfcn, Some(62));
        assert_eq!(signal.rxlev, Some(40)); // -70 dBm -> rxlev 40
    }

    #[test]
    fn gsm_without_arfcn_produces_no_observations() {
        let mut o = CellObserver::new();
        let ie = InformationElement::GSM(GsmMeta::default());
        let obs = o.observe(&ie, ts());
        assert!(obs.is_empty());
    }
}
