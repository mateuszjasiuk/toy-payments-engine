use rust_decimal::Decimal;

use crate::types::common::{ClientId, CsvRow, TxId};

#[derive(Debug)]
pub struct DepositTx {
    pub client_id: ClientId,
    pub tx_id: TxId,
    pub amount: Decimal,
}

// We allow the dead code as tx_id is never used in this impl.
// We keep it for consistecy though.
#[allow(dead_code)]
#[derive(Debug)]
pub struct WithdrawalTx {
    pub client_id: ClientId,
    pub tx_id: TxId,
    pub amount: Decimal,
}

#[derive(Debug)]
pub struct DisputeTx {
    pub client_id: ClientId,
    pub tx_id: TxId,
}

#[derive(Debug)]
pub struct ResolveTx {
    pub client_id: ClientId,
    pub tx_id: TxId,
}

#[derive(Debug)]
pub struct ChargebackTx {
    pub client_id: ClientId,
    pub tx_id: TxId,
}

#[derive(Debug)]
pub enum Tx {
    Deposit(DepositTx),
    Withdrawal(WithdrawalTx),
    Dispute(DisputeTx),
    Resolve(ResolveTx),
    Chargeback(ChargebackTx),
}

impl TryFrom<CsvRow> for Tx {
    type Error = ();

    fn try_from(value: CsvRow) -> Result<Self, Self::Error> {
        match value.r#type.as_str() {
            "deposit" => Ok(Tx::Deposit(DepositTx {
                client_id: value.client,
                tx_id: value.tx,
                amount: value.amount.ok_or(())?,
            })),
            "withdrawal" => Ok(Tx::Withdrawal(WithdrawalTx {
                client_id: value.client,
                tx_id: value.tx,
                amount: value.amount.ok_or(())?,
            })),
            "dispute" => Ok(Tx::Dispute(DisputeTx {
                client_id: value.client,
                tx_id: value.tx,
            })),
            "resolve" => Ok(Tx::Resolve(ResolveTx {
                client_id: value.client,
                tx_id: value.tx,
            })),
            "chargeback" => Ok(Tx::Chargeback(ChargebackTx {
                client_id: value.client,
                tx_id: value.tx,
            })),
            _ => Err(()),
        }
    }
}
