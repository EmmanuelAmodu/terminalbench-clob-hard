use std::fmt;

pub const LIMIT: u64 = 1_000_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}
impl Side {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "B" => Ok(Self::Buy),
            "S" => Ok(Self::Sell),
            _ => Err("side must be B or S".into()),
        }
    }
    pub fn opposite(self) -> Self {
        match self {
            Self::Buy => Self::Sell,
            Self::Sell => Self::Buy,
        }
    }
}
impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", if *self == Self::Buy { "B" } else { "S" })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    New {
        id: u64,
        side: Side,
        price: u64,
        qty: u64,
    },
    Amend {
        id: u64,
        qty: u64,
    },
    Cancel {
        id: u64,
    },
}
impl Command {
    pub fn parse(words: &[&str]) -> Result<Self, String> {
        match words {
            ["N", id, side, price, qty] => Ok(Self::New {
                id: number(id)?,
                side: Side::parse(side)?,
                price: number(price)?,
                qty: number(qty)?,
            }),
            ["A", id, qty] => Ok(Self::Amend {
                id: number(id)?,
                qty: number(qty)?,
            }),
            ["C", id] => Ok(Self::Cancel { id: number(id)? }),
            _ => Err("command must be N id side price qty, A id remaining_qty, or C id".into()),
        }
    }
}
impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::New {
                id,
                side,
                price,
                qty,
            } => write!(f, "N {id} {side} {price} {qty}"),
            Self::Amend { id, qty } => write!(f, "A {id} {qty}"),
            Self::Cancel { id } => write!(f, "C {id}"),
        }
    }
}
pub fn number(s: &str) -> Result<u64, String> {
    s.parse()
        .map_err(|_| format!("invalid unsigned integer: {s}"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Order {
    pub id: u64,
    pub side: Side,
    pub price: u64,
    pub remaining: u64,
    pub priority: u64,
    pub touched: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Execution {
    pub seq: u64,
    pub ordinal: u64,
    pub maker: u64,
    pub taker: u64,
    pub side: Side,
    pub price: u64,
    pub qty: u64,
}
impl Execution {
    pub fn id(&self) -> String {
        format!("{}.{}", self.seq, self.ordinal)
    }
    pub fn row(&self) -> String {
        format!(
            "EXEC {} {} {} {} {} {} {} {}",
            self.id(),
            self.seq,
            self.ordinal,
            self.maker,
            self.taker,
            self.side,
            self.price,
            self.qty
        )
    }
}
