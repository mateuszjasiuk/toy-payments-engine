use rust_decimal::Decimal;

use crate::types::common::ClientId;

pub struct Client {
    pub id: ClientId,
    pub available: Decimal,
    pub held: Decimal,
    pub total: Decimal,
    pub locked: bool,
}

impl Client {
    pub fn new(id: ClientId) -> Self {
        Client {
            id,
            available: Decimal::new(0, 0),
            held: Decimal::new(0, 0),
            total: Decimal::new(0, 0),
            locked: false,
        }
    }
}
