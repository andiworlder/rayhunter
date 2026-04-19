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
}
