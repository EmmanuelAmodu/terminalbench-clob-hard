use crate::model::{Command, Execution, Order, Side, LIMIT};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Book {
    pub orders: Vec<Order>,
    pub used_ids: BTreeSet<u64>,
}
impl Book {
    pub fn apply(&mut self, seq: u64, command: &Command) -> (String, Vec<Execution>) {
        match *command {
            Command::New {
                id,
                side,
                price,
                qty,
            } => {
                if id == 0 || price == 0 || price > LIMIT || qty == 0 || qty > LIMIT {
                    return ("INVALID".into(), vec![]);
                }
                if self.used_ids.contains(&id) {
                    return ("DUPLICATE_ORDER".into(), vec![]);
                }
                self.used_ids.insert(id);
                let mut remaining = qty;
                let mut executions = Vec::new();
                while remaining > 0 {
                    let maker_index = self
                        .orders
                        .iter()
                        .enumerate()
                        .filter(|(_, o)| {
                            o.side == side.opposite()
                                && match side {
                                    Side::Buy => o.price <= price,
                                    Side::Sell => o.price >= price,
                                }
                        })
                        .min_by_key(|(i, o)| {
                            (
                                if side == Side::Buy {
                                    o.price
                                } else {
                                    LIMIT - o.price
                                },
                                *i,
                            )
                        })
                        .map(|(i, _)| i);
                    let Some(i) = maker_index else {
                        break;
                    };
                    let maker = &mut self.orders[i];
                    let traded = remaining.min(maker.remaining);
                    remaining -= traded;
                    maker.remaining -= traded;
                    maker.touched = seq;
                    executions.push(Execution {
                        seq,
                        ordinal: executions.len() as u64,
                        maker: maker.id,
                        taker: id,
                        side,
                        price: maker.price,
                        qty: traded,
                    });
                    if maker.remaining == 0 {
                        self.orders.remove(i);
                    }
                }
                if remaining > 0 {
                    self.orders.push(Order {
                        id,
                        side,
                        price,
                        remaining,
                        priority: seq,
                        touched: seq,
                    });
                }
                ("OK".into(), executions)
            }
            Command::Amend { id, qty } => {
                if qty > LIMIT {
                    return ("INVALID".into(), vec![]);
                }
                let Some(i) = self.orders.iter().position(|o| o.id == id) else {
                    return ("UNKNOWN_ORDER".into(), vec![]);
                };
                if qty == 0 {
                    self.orders.remove(i);
                    return ("OK".into(), vec![]);
                }
                let old_qty = self.orders[i].remaining;
                if qty > old_qty {
                    let mut order = self.orders.remove(i);
                    order.remaining = qty;
                    order.priority = seq;
                    order.touched = seq;
                    self.orders.push(order);
                } else {
                    self.orders[i].remaining = qty;
                    self.orders[i].touched = seq;
                }
                ("OK".into(), vec![])
            }
            Command::Cancel { id } => {
                let Some(i) = self.orders.iter().position(|o| o.id == id) else {
                    return ("UNKNOWN_ORDER".into(), vec![]);
                };
                self.orders.remove(i);
                ("OK".into(), vec![])
            }
        }
    }
    pub fn rows(&self) -> Vec<String> {
        let mut sorted = self.orders.clone();
        sorted.sort_by_key(|o| o.id);
        let mut rows: Vec<String> = sorted
            .iter()
            .map(|o| {
                format!(
                    "BOOK {} {} {} {} {}",
                    o.id, o.side, o.price, o.remaining, o.priority
                )
            })
            .collect();
        let mut levels = BTreeMap::<(String, u64), Vec<String>>::new();
        for o in &self.orders {
            levels
                .entry((o.side.to_string(), o.price))
                .or_default()
                .push(o.id.to_string());
        }
        for ((side, price), ids) in levels {
            rows.push(format!("QUEUE {side} {price} {}", ids.join(",")));
        }
        rows
    }
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = BTreeSet::new();
        for o in &self.orders {
            if o.remaining == 0 || !ids.insert(o.id) || !self.used_ids.contains(&o.id) {
                return Err("invalid live order".into());
            }
        }
        for buy in self.orders.iter().filter(|o| o.side == Side::Buy) {
            if self
                .orders
                .iter()
                .any(|o| o.side == Side::Sell && o.price <= buy.price)
            {
                return Err("crossed resting book".into());
            }
        }
        Ok(())
    }
}
