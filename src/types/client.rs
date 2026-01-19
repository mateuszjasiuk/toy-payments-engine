use rust_decimal::{Decimal, prelude::Zero};

use crate::types::common::ClientId;

#[derive(Debug, serde::Serialize)]
pub struct Client {
    #[serde(rename = "client")]
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
            available: Decimal::zero(),
            held: Decimal::zero(),
            total: Decimal::zero(),
            locked: false,
        }
    }
}
