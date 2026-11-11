use crate::model::Execution;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default, Debug)]
pub struct Outbox {
    pub batches: BTreeMap<u64, Vec<Execution>>,
    pub confirmed: BTreeSet<String>,
}
impl Outbox {
    pub fn record(&mut self, seq: u64, executions: Vec<Execution>) -> Result<(), String> {
        if let Some(previous) = self.batches.get(&seq) {
            if previous != &executions {
                return Err("OUTBOX_CONFLICT".into());
            }
        } else {
            self.batches.insert(seq, executions);
        }
        Ok(())
    }
    pub fn pending(
        &self,
        through: u64,
        sent: &BTreeSet<String>,
        limit: usize,
        lane: Option<u64>,
    ) -> Vec<Execution> {
        self.batches
            .range(..=through)
            .flat_map(|(_, e)| e)
            .filter(|e| {
                !self.confirmed.contains(&e.id())
                    && !sent.contains(&e.id())
                    && lane.is_none_or(|lane| e.seq % 2 == lane)
            })
            .take(limit)
            .cloned()
            .collect()
    }
    pub fn confirm(&mut self, ids: Vec<String>) {
        self.confirmed.extend(ids);
    }
    pub fn receipt_frontier(&self, applied: u64, transport_prefix: u64) -> u64 {
        let mut credit = transport_prefix;
        let mut frontier = 0;
        for seq in 1..=applied {
            let Some(batch) = self.batches.get(&seq) else {
                break;
            };
            let required = batch.len() as u64;
            if credit < required {
                break;
            }
            credit -= required;
            frontier = seq;
        }
        frontier
    }
    pub fn rows(&self) -> Vec<String> {
        let mut rows = Vec::new();
        for (seq, batch) in &self.batches {
            rows.push(format!(
                "BATCH {} {} {}",
                seq,
                batch.len(),
                batch
                    .iter()
                    .filter(|e| self.confirmed.contains(&e.id()))
                    .count()
            ));
            for e in batch {
                if !self.confirmed.contains(&e.id()) {
                    rows.push(format!("PENDING {}", e.id()));
                }
            }
        }
        rows
    }
}
