use crate::model::Execution;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct Receipt {
    pub execution: String,
    pub consumed: bool,
    pub seq: u64,
}
#[derive(Default, Debug)]
pub struct Publisher {
    accepted: BTreeMap<String, Execution>,
    pub visible: Vec<Execution>,
    pub receipts: Vec<Receipt>,
    pub acknowledged_prefix: u64,
    pub retired: u64,
}
impl Publisher {
    pub fn publish(
        &mut self,
        active_term: u64,
        sender_term: u64,
        execution: &Execution,
    ) -> Result<(), String> {
        if active_term != sender_term {
            return Err("FENCED".into());
        }
        let id = execution.id();
        if let Some(previous) = self.accepted.get(&id) {
            if previous != execution {
                return Err("EXECUTION_CONFLICT".into());
            }
        } else {
            self.accepted.insert(id.clone(), execution.clone());
            self.visible.push(execution.clone());
        }
        self.receipts.push(Receipt {
            execution: id,
            consumed: false,
            seq: execution.seq,
        });
        Ok(())
    }
    pub fn take_receipts(&mut self, limit: usize, lane: Option<u64>) -> Vec<String> {
        let mut selected = Vec::new();
        for receipt in &mut self.receipts {
            if selected.len() == limit {
                break;
            }
            if !receipt.consumed && lane.is_none_or(|lane| receipt.seq % 2 == lane) {
                receipt.consumed = true;
                selected.push(receipt.execution.clone());
            }
        }
        self.acknowledged_prefix = self.receipts.iter().take_while(|r| r.consumed).count() as u64;
        selected
    }
    pub fn reclaim(&mut self, through: u64) {
        self.retired = self.retired.max(through);
        self.accepted.retain(|_, e| e.seq > self.retired);
    }
    pub fn rows(&self) -> Vec<String> {
        let mut rows = vec![format!(
            "RETENTION {} {}",
            self.retired,
            self.accepted.len()
        )];
        rows.extend(
            self.receipts
                .iter()
                .enumerate()
                .map(|(n, r)| format!("RECEIPT {} {} {}", n + 1, r.execution, u8::from(r.consumed)))
                .collect::<Vec<_>>(),
        );
        rows
    }
}
