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
    fn resolves_tmobile_usa() {
        let op = lookup_operator(Plmn { mcc: 310, mnc: 260, mnc_is_3_digit: true });
        assert_eq!(op, Some("T-Mobile USA"));
    }

    #[test]
    fn unknown_plmn_returns_none() {
        let op = lookup_operator(Plmn { mcc: 999, mnc: 99, mnc_is_3_digit: false });
        assert_eq!(op, None);
    }
}
