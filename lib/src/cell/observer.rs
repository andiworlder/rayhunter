use chrono::{DateTime, FixedOffset};
use deku::bitvec::*;
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
        lte: &crate::analysis::information_element::LteInformationElement,
        now: DateTime<FixedOffset>,
    ) -> Vec<CellObservation> {
        use crate::analysis::information_element::LteInformationElement as L;
        use telcom_parser::lte_rrc::{
            BCCH_DL_SCH_MessageType, BCCH_DL_SCH_MessageType_c1,
        };

        let mut out = Vec::new();

        if let L::BcchDlSch(sch) = lte
            && let BCCH_DL_SCH_MessageType::C1(BCCH_DL_SCH_MessageType_c1::SystemInformationBlockType1(sib1)) = &sch.message
        {
            let cid = sib1.cell_access_related_info.cell_identity.0
                .as_bitslice().load_be::<u32>();
            let tac = sib1.cell_access_related_info.tracking_area_code.0
                .as_bitslice().load_be::<u32>();
            let plmn = extract_first_plmn(&sib1.cell_access_related_info.plmn_identity_list.0);
            let identity = CellIdentity::Lte {
                plmn,
                tac: Some(tac),
                cid: Some(cid as u64),
                pci: None,
                earfcn: None,
            };
            out.push(CellObservation::Serving {
                identity,
                signal: SignalSample {
                    timestamp: Some(now),
                    ..Default::default()
                },
                timestamp: now,
            });
        }

        out
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

fn extract_first_plmn(
    list: &[telcom_parser::lte_rrc::PLMN_IdentityInfo],
) -> Option<crate::cell::Plmn> {
    let info = list.first()?;
    let mcc_digits = info.plmn_identity.mcc.as_ref()?;
    let mcc_vals: Vec<u16> = mcc_digits.0.iter().map(|d| d.0 as u16).collect();
    if mcc_vals.len() != 3 {
        return None;
    }
    let mcc = mcc_vals[0] * 100 + mcc_vals[1] * 10 + mcc_vals[2];

    let mnc_digits = &info.plmn_identity.mnc.0;
    let mnc_vals: Vec<u16> = mnc_digits.iter().map(|d| d.0 as u16).collect();
    let (mnc, is_3) = match mnc_vals.len() {
        2 => (mnc_vals[0] * 10 + mnc_vals[1], false),
        3 => (mnc_vals[0] * 100 + mnc_vals[1] * 10 + mnc_vals[2], true),
        _ => return None,
    };
    Some(crate::cell::Plmn {
        mcc,
        mnc,
        mnc_is_3_digit: is_3,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::information_element::{GsmMeta, InformationElement, UmtsMeta};
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

    #[test]
    fn lte_observer_returns_empty_for_non_sib_ie() {
        // A non-SIB1 LTE IE (UMTS path here) should produce zero observations
        // without panicking.
        let mut o = CellObserver::new();
        let ie = InformationElement::UMTS(UmtsMeta::default());
        assert_eq!(o.observe(&ie, ts()).len(), 0);
    }
}
