use crate::{
    book::Book,
    journal::{Entry, Journal},
    model::{number, Command, Execution},
    outbox::Outbox,
    publisher::Publisher,
    snapshot::SnapshotStore,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fault {
    #[default]
    None,
    AfterCommit,
    AfterApply,
    AfterAck,
}
#[derive(Debug)]
pub struct Replica {
    pub book: Book,
    pub applied: u64,
    pub acknowledged: u64,
    pub ready: bool,
    pub sent: BTreeSet<String>,
}
impl Default for Replica {
    fn default() -> Self {
        Self {
            book: Book::default(),
            applied: 0,
            acknowledged: 0,
            ready: true,
            sent: BTreeSet::new(),
        }
    }
}
#[derive(Debug)]
pub struct Cluster {
    pub journal: Journal,
    pub snapshot: SnapshotStore,
    pub outbox: Outbox,
    pub publisher: Publisher,
    pub worker: Option<Replica>,
    pub term: u64,
    pub fault: Fault,
    pub publish_fault: Option<u64>,
    pub replay_fault: Option<u64>,
    pub auto: bool,
    pub results: BTreeMap<u64, String>,
    pub notices: Vec<String>,
}
impl Default for Cluster {
    fn default() -> Self {
        Self {
            journal: Journal::default(),
            snapshot: SnapshotStore::default(),
            outbox: Outbox::default(),
            publisher: Publisher::default(),
            worker: Some(Replica::default()),
            term: 1,
            fault: Fault::None,
            publish_fault: None,
            replay_fault: None,
            auto: true,
            results: BTreeMap::new(),
            notices: vec![],
        }
    }
}
impl Cluster {
    fn apply_entry(&mut self, entry: &Entry) -> Result<(), String> {
        let replica = self.worker.as_mut().ok_or("worker unavailable")?;
        if entry.seq != replica.applied + 1 {
            return Err("nonconsecutive application".into());
        }
        let (result, executions) = replica.book.apply(entry.seq, &entry.command);
        self.outbox.record(entry.seq, executions)?;
        replica.applied = entry.seq;
        self.results.insert(entry.seq, result);
        Ok(())
    }
    fn lose_worker(&mut self) {
        self.worker = None;
    }
    fn publication_gate(&mut self, term: u64) -> bool {
        if term != self.term {
            self.notices.push("FENCED_PUBLICATION".into());
            false
        } else if self.worker.is_none() {
            self.notices.push("UNAVAILABLE".into());
            false
        } else {
            true
        }
    }
    pub fn drain(&mut self, term: u64, limit: usize) -> Result<(), String> {
        self.drain_lane(term, limit, None)
    }
    pub fn drain_lane(&mut self, term: u64, limit: usize, lane: Option<u64>) -> Result<(), String> {
        if !self.publication_gate(term) {
            return Ok(());
        }
        let worker = self.worker.as_ref().unwrap();
        let selected = self
            .outbox
            .pending(worker.applied, &worker.sent, limit, lane);
        let fault = self.publish_fault.take();
        for (index, execution) in selected.iter().enumerate() {
            self.publisher.publish(self.term, term, execution)?;
            self.worker.as_mut().unwrap().sent.insert(execution.id());
            if fault == Some(index as u64 + 1) {
                self.lose_worker();
                return Ok(());
            }
        }
        Ok(())
    }
    pub fn confirm(&mut self, term: u64, limit: usize) -> Result<(), String> {
        self.confirm_lane(term, limit, None)
    }
    pub fn confirm_lane(
        &mut self,
        term: u64,
        limit: usize,
        lane: Option<u64>,
    ) -> Result<(), String> {
        if !self.publication_gate(term) {
            return Ok(());
        }
        let ids = self.publisher.take_receipts(limit, lane);
        self.outbox.confirm(ids);
        Ok(())
    }
    fn finish_automatic(&mut self) -> Result<(), String> {
        if self.auto {
            self.drain(self.term, usize::MAX)?;
            if self.worker.is_some() {
                self.confirm(self.term, usize::MAX)?;
            }
        }
        Ok(())
    }
    pub fn submit(&mut self, term: u64, request: u64, command: Command) -> Result<(), String> {
        if term != self.term {
            self.notices.push("FENCED_COMMAND".into());
            return Ok(());
        }
        let Some(worker) = self.worker.as_ref() else {
            self.notices.push("UNAVAILABLE".into());
            return Ok(());
        };
        if !worker.ready {
            self.notices.push("RECOVERING".into());
            return Ok(());
        }
        let entry = match self.journal.append(term, request, command) {
            Ok(Some(e)) => e,
            Ok(None) => {
                self.notices.push("DUPLICATE_REQUEST".into());
                return Ok(());
            }
            Err(e) => {
                self.notices.push(e);
                return Ok(());
            }
        };
        let fault = std::mem::take(&mut self.fault);
        if fault == Fault::AfterCommit {
            self.lose_worker();
            return Ok(());
        }
        self.apply_entry(&entry)?;
        if fault == Fault::AfterApply {
            self.lose_worker();
            return Ok(());
        }
        self.finish_automatic()?;
        if let Some(worker) = self.worker.as_mut() {
            worker.acknowledged = entry.seq;
            if fault == Fault::AfterAck {
                self.lose_worker();
            }
        }
        Ok(())
    }
    pub fn promote(&mut self, paused: bool) -> Result<(), String> {
        self.term = self.term.checked_add(1).ok_or("term exhausted")?;
        let (book, boundary) = self.snapshot.restore()?;
        self.worker = Some(Replica {
            book,
            applied: boundary,
            acknowledged: boundary,
            ready: boundary == self.journal.committed,
            sent: BTreeSet::new(),
        });
        if !paused {
            self.recover(usize::MAX)?;
        }
        Ok(())
    }
    pub fn recover(&mut self, limit: usize) -> Result<(), String> {
        let applied = self
            .worker
            .as_ref()
            .ok_or("cannot recover dead worker")?
            .applied;
        let fault = self.replay_fault.take();
        for (index, entry) in self.journal.tail(applied)?.iter().take(limit).enumerate() {
            self.apply_entry(entry)?;
            self.worker.as_mut().unwrap().acknowledged = entry.seq;
            if fault == Some(index as u64 + 1) {
                self.lose_worker();
                return Ok(());
            }
        }
        let worker = self.worker.as_mut().unwrap();
        worker.ready = worker.applied == self.journal.committed;
        if worker.ready {
            self.finish_automatic()?;
        }
        Ok(())
    }
    pub fn checkpoint(&mut self, incremental: bool) -> Result<(), String> {
        let worker = self.worker.as_ref().ok_or("cannot snapshot dead worker")?;
        if worker.applied != worker.acknowledged {
            return Err("snapshot requires acknowledged application".into());
        }
        if incremental {
            self.snapshot.incremental(&worker.book, worker.applied)?;
        } else {
            self.snapshot.full(&worker.book, worker.applied);
        }
        Ok(())
    }
    pub fn compact(&mut self) -> Result<(), String> {
        let worker = self.worker.as_ref().ok_or("cannot compact dead worker")?;
        let through = self
            .outbox
            .receipt_frontier(worker.applied, self.publisher.acknowledged_prefix);
        self.snapshot.compact()?;
        self.publisher.reclaim(through);
        Ok(())
    }
    pub fn stale_publish(&mut self, term: u64) -> Result<(), String> {
        let execution = self
            .publisher
            .visible
            .first()
            .cloned()
            .ok_or("no execution for stale probe")?;
        if !self.publication_gate(term) {
            return Ok(());
        }
        if !self.outbox.confirmed.contains(&execution.id()) {
            self.publisher.publish(self.term, term, &execution)?;
        }
        Ok(())
    }
    pub fn run(&mut self, input: &str) -> Result<(), String> {
        for (index, line) in input.lines().enumerate() {
            let line = line.split('#').next().unwrap().trim();
            if line.is_empty() {
                continue;
            }
            let words: Vec<_> = line.split_whitespace().collect();
            let outcome = (|| match words.as_slice() {
                ["SUBMIT", term, request, rest @ ..] => {
                    self.submit(number(term)?, number(request)?, Command::parse(rest)?)
                }
                ["SNAPSHOT"] | ["SNAPSHOT", "FULL"] => self.checkpoint(false),
                ["SNAPSHOT", "DELTA"] => self.checkpoint(true),
                ["COMPACT"] => self.compact(),
                ["TRIM", through] => self.journal.trim(number(through)?, self.snapshot.boundary),
                ["PROMOTE"] => self.promote(false),
                ["PROMOTE", "PAUSE"] => self.promote(true),
                ["RECOVER", count] => self.recover(count_value(count)?),
                ["MODE", "AUTO"] => {
                    self.auto = true;
                    Ok(())
                }
                ["MODE", "DEFER"] => {
                    self.auto = false;
                    Ok(())
                }
                ["DRAIN", term, count] => self.drain(number(term)?, count_value(count)?),
                ["DRAIN", term, count, lane] if number(lane)? < 2 => {
                    self.drain_lane(number(term)?, count_value(count)?, Some(number(lane)?))
                }
                ["CONFIRM", term, count] => self.confirm(number(term)?, count_value(count)?),
                ["CONFIRM", term, count, lane] if number(lane)? < 2 => {
                    self.confirm_lane(number(term)?, count_value(count)?, Some(number(lane)?))
                }
                ["CRASH"] => {
                    self.lose_worker();
                    Ok(())
                }
                ["STALE_PUBLISH", term] => self.stale_publish(number(term)?),
                ["FAULT", "NONE"] => {
                    self.fault = Fault::None;
                    self.publish_fault = None;
                    self.replay_fault = None;
                    Ok(())
                }
                ["FAULT", "COMMIT"] => {
                    self.fault = Fault::AfterCommit;
                    Ok(())
                }
                ["FAULT", "APPLY"] => {
                    self.fault = Fault::AfterApply;
                    Ok(())
                }
                ["FAULT", "ACK"] => {
                    self.fault = Fault::AfterAck;
                    Ok(())
                }
                ["FAULT", "PUBLISH", n] if number(n)? > 0 => {
                    self.publish_fault = Some(number(n)?);
                    Ok(())
                }
                ["FAULT", "RECOVER", n] if number(n)? > 0 => {
                    self.replay_fault = Some(number(n)?);
                    Ok(())
                }
                _ => Err("unknown operation".into()),
            })();
            outcome.map_err(|e| format!("line {}: {e}", index + 1))?;
        }
        Ok(())
    }
    pub fn render(&self) -> String {
        let mut rows = self.journal.rows();
        for (seq, result) in &self.results {
            rows.push(format!("RESULT {seq} {result}"));
        }
        rows.extend(self.publisher.visible.iter().map(Execution::row));
        if let Some(worker) = &self.worker {
            rows.extend(worker.book.rows());
        }
        let (applied, ack, role) = self
            .worker
            .as_ref()
            .map(|w| {
                (
                    w.applied,
                    w.acknowledged,
                    if w.ready { "LIVE" } else { "RECOVERING" },
                )
            })
            .unwrap_or((0, 0, "DEAD"));
        rows.push(format!(
            "STATUS {} {} {} {} {} {}",
            self.term,
            self.journal.committed,
            applied,
            ack,
            self.snapshot.boundary,
            u8::from(self.worker.is_some())
        ));
        rows.push(format!("ROLE {role}"));
        rows.push(format!(
            "CONFIG {}",
            if self.auto { "AUTO" } else { "DEFER" }
        ));
        rows.push(format!("TRIMMED {}", self.journal.trimmed));
        rows.extend(self.outbox.rows());
        rows.extend(self.publisher.rows());
        rows.extend(self.notices.iter().map(|n| format!("NOTICE {n}")));
        format!("{}\n", rows.join("\n"))
    }
}
fn count_value(text: &str) -> Result<usize, String> {
    usize::try_from(number(text)?).map_err(|_| "count exceeds platform range".into())
}
