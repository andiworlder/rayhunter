use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

/// Decode 3GPP TS 36.133 Table 9.1.4-1 `rsrpResult` (0..97) to dBm.
pub fn decode_lte_rsrp(raw: u8) -> i16 {
    // rsrp_dBm = rsrpResult - 140, clamped 0..97 -> -140..-43
    (raw.min(97) as i16) - 140
}

/// Decode 3GPP TS 36.133 Table 9.1.7-1 `rsrqResult` (0..34) to dB.
///
/// The standard maps rsrqResult to half-dB steps from -19.5 to -3.0.
/// We return nearest integer dB.
pub fn decode_lte_rsrq(raw: u8) -> i8 {
    let v = raw.min(34) as i16;
    ((v - 39) / 2) as i8
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SignalSample {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<FixedOffset>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rsrp_dbm: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rsrq_db: Option<i8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rscp_dbm: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ecno_db: Option<i8>,
    #[serde(skip_serializing_if = "Option::is_none")]
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

    #[test]
    fn signal_sample_is_empty_when_all_none() {
        let s = SignalSample::default();
        assert!(s.is_empty());
    }
}
