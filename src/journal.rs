use crate::model::Command;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct Entry {
    pub seq: u64,
    pub term: u64,
    pub request: u64,
    pub command: Command,
}
#[derive(Default, Debug)]
pub struct Journal {
    pub entries: Vec<Entry>,
    pub committed: u64,
    pub trimmed: u64,
    requests: BTreeMap<u64, Command>,
}
impl Journal {
    pub fn append(
        &mut self,
        term: u64,
        request: u64,
        command: Command,
    ) -> Result<Option<Entry>, String> {
        if let Some(previous) = self.requests.get(&request) {
            return if previous == &command {
                Ok(None)
            } else {
                Err("REQUEST_CONFLICT".into())
            };
        }
        self.committed = self.committed.checked_add(1).ok_or("journal exhausted")?;
        let entry = Entry {
            seq: self.committed,
            term,
            request,
            command,
        };
        self.requests.insert(request, entry.command.clone());
        self.entries.push(entry.clone());
        Ok(Some(entry))
    }
    pub fn tail(&self, after: u64) -> Result<Vec<Entry>, String> {
        if after < self.trimmed {
            return Err("checkpoint precedes retained journal".into());
        }
        Ok(self
            .entries
            .iter()
            .filter(|e| e.seq > after)
            .cloned()
            .collect())
    }
    pub fn trim(&mut self, through: u64, checkpoint: u64) -> Result<(), String> {
        if through < self.trimmed || through > checkpoint {
            return Err("invalid trim boundary".into());
        }
        self.entries.retain(|e| e.seq > through);
        self.trimmed = through;
        Ok(())
    }
    pub fn rows(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|e| format!("JOURNAL {} {} {} {}", e.seq, e.term, e.request, e.command))
            .collect()
    }
}
