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
    avg_accum: HashMap<CellKey, (i64, u32)>, // (sum, count)
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

    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }

    pub fn apply(&mut self, obs: &CellObservation) {
        let (identity, signal, is_serving, ts) = match obs {
            CellObservation::Serving { identity, signal, timestamp } => {
                (identity, signal, true, *timestamp)
            }
            CellObservation::Neighbor { identity, signal, timestamp } => {
                (identity, signal, false, *timestamp)
            }
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

        entry.identity.merge_from(identity);
        if entry.operator_name.is_none() {
            entry.operator_name = entry
                .identity
                .plmn()
                .and_then(lookup_operator)
                .map(|s| s.to_string());
        }
        entry.last_seen = ts;
        entry.observation_count += 1;
        if is_serving {
            entry.is_serving_ever = true;
        }
        entry.current_signal = Some(signal.clone());

        update_min_max(&mut entry.signal_min, &mut entry.signal_max, signal);

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
                if self.serving_rsrp_buffer.len() == self.buffer_capacity {
                    self.serving_rsrp_buffer.pop_front();
                }
                self.serving_rsrp_buffer.push_back((ts, rsrp));
            }
        }

        let neigh_count = self
            .by_key
            .values()
            .filter(|a| !a.is_serving_ever)
            .count() as u32;
        if self.neighbor_count_buffer.len() == self.buffer_capacity {
            self.neighbor_count_buffer.pop_front();
        }
        self.neighbor_count_buffer.push_back((ts, neigh_count));
    }

    pub fn current_context(&self, max_neighbors: usize) -> CellContext {
        let serving = self
            .current_serving_key
            .as_ref()
            .and_then(|k| self.by_key.get(k));
        let serving_id = serving.map(|s| s.identity.clone());
        let serving_sig = serving.and_then(|s| s.current_signal.clone());
        let serving_op = serving.and_then(|s| s.operator_name.clone());

        let mut neigh: Vec<&CellAggregate> = self
            .by_key
            .values()
            .filter(|a| !a.is_serving_ever && a.current_signal.is_some())
            .collect();
        neigh.sort_by_key(|a| {
            -(a.current_signal
                .as_ref()
                .and_then(|s| s.rsrp_dbm)
                .unwrap_or(i16::MIN) as i32)
        });
        neigh.truncate(max_neighbors);

        CellContext {
            serving: serving_id,
            serving_signal: serving_sig,
            serving_operator: serving_op,
            neighbors: neigh
                .into_iter()
                .map(|a| NeighborSnapshot {
                    identity: a.identity.clone(),
                    signal: a.current_signal.clone().unwrap_or_default(),
                    operator_name: a.operator_name.clone(),
                })
                .collect(),
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
}

fn update_min_max(
    min: &mut Option<SignalSample>,
    max: &mut Option<SignalSample>,
    s: &SignalSample,
) {
    if let Some(rsrp) = s.rsrp_dbm {
        if min.as_ref().and_then(|m| m.rsrp_dbm).is_none_or(|v| rsrp < v) {
            *min = Some(s.clone());
        }
        if max.as_ref().and_then(|m| m.rsrp_dbm).is_none_or(|v| rsrp > v) {
            *max = Some(s.clone());
        }
    }
    if let Some(rxl) = s.rxlev {
        if min.as_ref().and_then(|m| m.rxlev).is_none_or(|v| rxl < v) {
            *min = Some(s.clone());
        }
        if max.as_ref().and_then(|m| m.rxlev).is_none_or(|v| rxl > v) {
            *max = Some(s.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Plmn;
    use chrono::TimeZone;

    fn ts() -> DateTime<FixedOffset> {
        FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(2026, 4, 19, 12, 0, 0)
            .unwrap()
    }

    #[test]
    fn serving_observation_populates_store() {
        let mut s = CellStore::new(120);
        let ob = CellObservation::Serving {
            identity: CellIdentity::Lte {
                plmn: Some(Plmn { mcc: 510, mnc: 11, mnc_is_3_digit: false }),
                tac: Some(1280),
                cid: Some(23401),
                pci: None,
                earfcn: None,
            },
            signal: SignalSample {
                rsrp_dbm: Some(-79),
                rsrq_db: Some(-9),
                ..Default::default()
            },
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
        for pci in [301u16, 143, 288] {
            s.apply(&CellObservation::Neighbor {
                identity: CellIdentity::Lte {
                    plmn: None,
                    tac: None,
                    cid: None,
                    pci: Some(pci),
                    earfcn: Some(1350),
                },
                signal: SignalSample {
                    rsrp_dbm: Some(-85 - (pci as i16 % 5)),
                    ..Default::default()
                },
                timestamp: ts(),
            });
        }
        let ctx = s.current_context(2);
        assert_eq!(ctx.neighbors.len(), 2);
        assert!(
            ctx.neighbors[0].signal.rsrp_dbm.unwrap()
                >= ctx.neighbors[1].signal.rsrp_dbm.unwrap()
        );
    }

    #[test]
    fn reset_clears_store() {
        let mut s = CellStore::new(120);
        s.apply(&CellObservation::Serving {
            identity: CellIdentity::Lte {
                plmn: Some(Plmn { mcc: 510, mnc: 11, mnc_is_3_digit: false }),
                tac: Some(1), cid: Some(1), pci: None, earfcn: None,
            },
            signal: SignalSample::default(),
            timestamp: ts(),
        });
        assert_eq!(s.len(), 1);
        s.reset();
        assert_eq!(s.len(), 0);
        assert!(s.serving_rsrp_history().is_empty());
    }
}
