use rust_decimal::Decimal;

pub type ClientId = u16;
pub type TxId = u32;

#[derive(Debug, serde::Deserialize)]
pub struct CsvRow {
    pub r#type: String,
    pub client: ClientId,
    pub tx: TxId,
    pub amount: Option<Decimal>,
}
