use crate::{
    book::Book,
    model::{number, Order, Side},
};
use std::collections::{BTreeMap, BTreeSet};

fn encode_order(o: &Order) -> String {
    format!(
        "{} {} {} {} {} {}",
        o.id, o.side, o.price, o.remaining, o.priority, o.touched
    )
}
fn decode_order(fields: &[&str]) -> Result<Order, String> {
    match fields {
        [id, side, price, remaining, priority, touched] => Ok(Order {
            id: number(id)?,
            side: Side::parse(side)?,
            price: number(price)?,
            remaining: number(remaining)?,
            priority: number(priority)?,
            touched: number(touched)?,
        }),
        _ => Err("bad order image".into()),
    }
}
#[derive(Debug, Clone)]
pub struct Image {
    pub bytes: String,
}
impl Image {
    pub fn capture(book: &Book, boundary: u64) -> Self {
        let mut text = format!("BASE2 {boundary}\n");
        for id in &book.used_ids {
            text.push_str(&format!("U {id}\n"));
        }
        let mut records = book.orders.clone();
        records.sort_by_key(|o| o.id);
        for order in records {
            text.push_str(&format!("O {}\n", encode_order(&order)));
        }
        for order in &book.orders {
            text.push_str(&format!("Q {}\n", order.id));
        }
        Self { bytes: text }
    }
    pub fn restore(&self) -> Result<(Book, u64), String> {
        let mut lines = self.bytes.lines();
        let header: Vec<_> = lines
            .next()
            .ok_or("empty base")?
            .split_whitespace()
            .collect();
        let boundary = match header.as_slice() {
            ["BASE2", n] => number(n)?,
            _ => return Err("bad base version".into()),
        };
        let mut records = BTreeMap::new();
        let mut queue = Vec::new();
        let mut used = BTreeSet::new();
        for line in lines {
            let fields: Vec<_> = line.split_whitespace().collect();
            match fields.as_slice() {
                ["U", id] => {
                    used.insert(number(id)?);
                }
                ["O", rest @ ..] => {
                    let o = decode_order(rest)?;
                    if records.insert(o.id, o).is_some() {
                        return Err("duplicate image row".into());
                    }
                }
                ["Q", id] => queue.push(number(id)?),
                _ => return Err("bad image row".into()),
            }
        }
        let mut orders = Vec::new();
        for id in queue {
            orders.push(records.remove(&id).ok_or("bad queue membership")?);
        }
        if !records.is_empty() {
            return Err("unplaced image record".into());
        }
        let book = Book {
            orders,
            used_ids: used,
        };
        book.validate()?;
        Ok((book, boundary))
    }
}
#[derive(Debug, Clone)]
pub struct Delta {
    pub bytes: String,
}
impl Delta {
    pub fn capture(before: &Book, after: &Book, from: u64, through: u64) -> Self {
        let mut text = format!("DELTA2 {from} {through}\n");
        for id in after.used_ids.difference(&before.used_ids) {
            text.push_str(&format!("U {id}\n"));
        }
        let old: BTreeMap<_, _> = before.orders.iter().map(|o| (o.id, o)).collect();
        let live: BTreeSet<_> = after.orders.iter().map(|o| o.id).collect();
        for id in old.keys().filter(|id| !live.contains(id)) {
            text.push_str(&format!("D {id}\n"));
        }
        let mut records = after.orders.clone();
        records.sort_by_key(|o| o.id);
        for o in records {
            if old.get(&o.id).copied() != Some(&o) {
                text.push_str(&format!("R {}\n", encode_order(&o)));
            }
        }
        let mut previous = BTreeMap::new();
        for o in &after.orders {
            let key = (o.side.to_string(), o.price);
            let anchor = previous.get(&key).copied().unwrap_or(0);
            if old.get(&o.id).is_none_or(|old| old.priority != o.priority) {
                text.push_str(&format!("P {} {}\n", o.id, anchor));
            }
            previous.insert(key, o.id);
        }
        Self { bytes: text }
    }
    fn install_record(book: &mut Book, order: Order) {
        book.orders.retain(|previous| previous.id != order.id);
        book.orders.push(order);
    }
    pub fn apply(&self, book: &mut Book, boundary: u64) -> Result<u64, String> {
        let mut lines = self.bytes.lines();
        let header: Vec<_> = lines
            .next()
            .ok_or("empty delta")?
            .split_whitespace()
            .collect();
        let through = match header.as_slice() {
            ["DELTA2", from, to] if number(from)? == boundary && number(to)? >= boundary => {
                number(to)?
            }
            _ => return Err("delta chain mismatch".into()),
        };
        let mut placements = Vec::new();
        for line in lines {
            let fields: Vec<_> = line.split_whitespace().collect();
            match fields.as_slice() {
                ["U", id] => {
                    book.used_ids.insert(number(id)?);
                }
                ["D", id] => {
                    let id = number(id)?;
                    book.orders.retain(|o| o.id != id);
                }
                ["R", rest @ ..] => Self::install_record(book, decode_order(rest)?),
                ["P", id, anchor] => placements.push((number(id)?, number(anchor)?)),
                _ => return Err("bad delta row".into()),
            }
        }
        for (id, anchor) in placements {
            let position = book
                .orders
                .iter()
                .position(|o| o.id == id)
                .ok_or("missing placement member")?;
            let order = book.orders.remove(position);
            let index = if anchor == 0 {
                book.orders
                    .iter()
                    .position(|o| o.side == order.side && o.price == order.price)
                    .unwrap_or(book.orders.len())
            } else {
                book.orders
                    .iter()
                    .position(|o| o.id == anchor && o.side == order.side && o.price == order.price)
                    .ok_or("missing placement anchor")?
                    + 1
            };
            book.orders.insert(index, order);
        }
        book.validate()?;
        Ok(through)
    }
}
#[derive(Debug, Clone)]
pub struct SnapshotStore {
    pub base: Image,
    pub deltas: Vec<Delta>,
    pub boundary: u64,
}
impl Default for SnapshotStore {
    fn default() -> Self {
        Self {
            base: Image::capture(&Book::default(), 0),
            deltas: vec![],
            boundary: 0,
        }
    }
}
impl SnapshotStore {
    pub fn restore(&self) -> Result<(Book, u64), String> {
        let (mut book, mut boundary) = self.base.restore()?;
        for delta in &self.deltas {
            boundary = delta.apply(&mut book, boundary)?;
        }
        if boundary != self.boundary {
            return Err("snapshot watermark mismatch".into());
        }
        Ok((book, boundary))
    }
    pub fn full(&mut self, book: &Book, boundary: u64) {
        self.base = Image::capture(book, boundary);
        self.deltas.clear();
        self.boundary = boundary;
    }
    pub fn incremental(&mut self, book: &Book, boundary: u64) -> Result<(), String> {
        let (before, from) = self.restore()?;
        self.deltas
            .push(Delta::capture(&before, book, from, boundary));
        self.boundary = boundary;
        Ok(())
    }
    pub fn compact(&mut self) -> Result<(), String> {
        let (book, boundary) = self.restore()?;
        self.full(&book, boundary);
        Ok(())
    }
}
