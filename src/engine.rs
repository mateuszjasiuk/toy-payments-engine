use std::collections::HashMap;

use crate::types::{
    client::Client,
    common::{ClientId, TxId},
    transactions::{ChargebackTx, DepositTx, DisputeTx, ResolveTx, Tx, WithdrawalTx},
};

#[derive(Debug, PartialEq, Eq)]
enum DepositStatus {
    Normal,
    UnderDispute,
    Resolved,
    ChargedBack,
}

pub struct Engine {
    clients: HashMap<ClientId, Client>,
    deposits: HashMap<TxId, (DepositTx, DepositStatus)>,
}

impl Engine {
    pub fn new() -> Self {
        Engine {
            clients: HashMap::new(),
            deposits: HashMap::new(),
        }
    }

    pub fn clients(&self) -> &HashMap<ClientId, Client> {
        &self.clients
    }

    pub fn process_tx(&mut self, tx: Tx) {
        match tx {
            Tx::Deposit(deposit_tx) => {
                self.process_deposit(deposit_tx);
            }
            Tx::Withdrawal(withdrawal_tx) => {
                self.process_withdrawal(withdrawal_tx);
            }
            Tx::Dispute(dispute_tx) => {
                self.process_dispute(dispute_tx);
            }
            Tx::Resolve(resolve_tx) => {
                self.process_resolve(resolve_tx);
            }
            Tx::Chargeback(chargeback_tx) => {
                self.process_chargeback(chargeback_tx);
            }
        }
    }

    fn process_deposit(&mut self, deposit_tx: DepositTx) {
        let client = self
            .clients
            .entry(deposit_tx.client_id)
            .or_insert(Client::new(deposit_tx.client_id));

        if client.locked {
            return; // Account is locked
        }

        client.available += deposit_tx.amount;
        client.total += deposit_tx.amount;

        // Spec claims that the ids are unique, but just to be sure
        self.deposits
            .entry(deposit_tx.tx_id)
            .or_insert((deposit_tx, DepositStatus::Normal));
    }

    fn process_withdrawal(&mut self, withdrawal_tx: WithdrawalTx) {
        let Some(client) = self.clients.get_mut(&withdrawal_tx.client_id) else {
            return; // Client doesn't exist
        };

        if client.locked {
            return; // Account is locked
        }

        if client.available < withdrawal_tx.amount {
            return; // Insufficient funds
        }

        client.available -= withdrawal_tx.amount;
        client.total -= withdrawal_tx.amount;
    }

    fn process_dispute(&mut self, dispute_tx: DisputeTx) {
        let Some(client) = self.clients.get_mut(&dispute_tx.client_id) else {
            return; // Client doesn't exist
        };

        let Some((deposit_tx, deposit_status)) = self.deposits.get_mut(&dispute_tx.tx_id) else {
            return; // Corresponding deposit doesn't exist
        };

        if dispute_tx.client_id != deposit_tx.client_id {
            return; // Dispute client doesn't match deposit client
        }

        if *deposit_status != DepositStatus::Normal {
            return; // Deposit is not in a state that can be disputed
        }

        *deposit_status = DepositStatus::UnderDispute;
        // Available can go negative if funds were already withdrawn (fraud scenario)
        client.available -= deposit_tx.amount;
        client.held += deposit_tx.amount;
    }

    fn process_resolve(&mut self, resolve_tx: ResolveTx) {
        let Some(client) = self.clients.get_mut(&resolve_tx.client_id) else {
            return; // Client doesn't exist
        };

        let Some((deposit_tx, deposit_status)) = self.deposits.get_mut(&resolve_tx.tx_id) else {
            return; // Corresponding deposit doesn't exist
        };

        if resolve_tx.client_id != deposit_tx.client_id {
            return; // Dispute client doesn't match deposit client
        }

        if *deposit_status != DepositStatus::UnderDispute {
            return; // Deposit is not in a state that can be resolved
        }

        *deposit_status = DepositStatus::Resolved;
        client.available += deposit_tx.amount;
        client.held -= deposit_tx.amount;
    }

    fn process_chargeback(&mut self, chargeback_tx: ChargebackTx) {
        let Some(client) = self.clients.get_mut(&chargeback_tx.client_id) else {
            return; // Client doesn't exist
        };

        let Some((deposit_tx, deposit_status)) = self.deposits.get_mut(&chargeback_tx.tx_id) else {
            return; // Corresponding deposit doesn't exist
        };

        if chargeback_tx.client_id != deposit_tx.client_id {
            return; // Dispute client doesn't match deposit client
        }

        if *deposit_status != DepositStatus::UnderDispute {
            return; // Deposit is not in a state that can be charged back
        }

        *deposit_status = DepositStatus::ChargedBack;
        client.total -= deposit_tx.amount;
        client.held -= deposit_tx.amount;
        client.locked = true;
    }
}

#[cfg(test)]
mod tests {
    use crate::types::common::CsvRow;

    use super::*;
    use rust_decimal_macros::dec;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_process_deposit_new_client() {
        let mut engine = Engine::new();

        let deposit = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(100.0),
        };

        engine.process_deposit(deposit);

        let client = engine.clients.get(&1).unwrap();
        assert_eq!(client.available, dec!(100.0));
        assert_eq!(client.total, dec!(100.0));
        assert_eq!(client.held, dec!(0.0));
        assert!(engine.deposits.contains_key(&1));
    }

    #[test]
    fn test_process_deposit_existing_client() {
        let mut engine = Engine::new();

        let deposit1 = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(50.0),
        };

        let deposit2 = DepositTx {
            client_id: 1,
            tx_id: 2,
            amount: dec!(75.0),
        };

        engine.process_deposit(deposit1);
        engine.process_deposit(deposit2);

        let client = engine.clients.get(&1).unwrap();
        assert_eq!(client.available, dec!(125.0));
        assert_eq!(client.total, dec!(125.0));
        assert_eq!(client.held, dec!(0));
        assert_eq!(engine.deposits.len(), 2);
    }

    #[test]
    fn test_process_withdrawal_no_client() {
        let mut engine = Engine::new();

        let withdrawal = WithdrawalTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(50.0),
        };

        engine.process_withdrawal(withdrawal);

        let client = engine.clients.get(&1);
        assert!(client.is_none());
    }

    #[test]
    fn test_process_withdrawal_existing_client_with_balance() {
        let mut engine = Engine::new();

        let deposit = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(100.0),
        };

        let withdrawal = WithdrawalTx {
            client_id: 1,
            tx_id: 2,
            amount: dec!(50.0),
        };

        engine.process_deposit(deposit);
        engine.process_withdrawal(withdrawal);

        let client = engine.clients.get(&1).unwrap();
        assert_eq!(client.available, dec!(50.0));
        assert_eq!(client.total, dec!(50.0));
        assert!(engine.deposits.contains_key(&1));
    }

    #[test]
    fn test_process_withdrawal_existing_client_without_balance() {
        let mut engine = Engine::new();

        let deposit = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(10.0),
        };

        let withdrawal = WithdrawalTx {
            client_id: 1,
            tx_id: 2,
            amount: dec!(99.0),
        };

        engine.process_deposit(deposit);
        engine.process_withdrawal(withdrawal);

        let client = engine.clients.get(&1).unwrap();
        assert_eq!(client.available, dec!(10.0));
        assert_eq!(client.total, dec!(10.0));
        assert!(engine.deposits.contains_key(&1));
    }

    #[test]
    fn test_process_dispute_no_deposit() {
        let mut engine = Engine::new();

        let deposit = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(10.0),
        };

        let dispute = DisputeTx {
            client_id: 1,
            tx_id: 2,
        };

        engine.process_deposit(deposit);
        engine.process_dispute(dispute);

        let client = engine.clients.get(&1).unwrap();
        assert_eq!(client.available, dec!(10.0));
        assert_eq!(client.total, dec!(10.0));
        assert!(engine.deposits.contains_key(&1));
        assert!(!engine.deposits.contains_key(&2));

        let (_, status) = engine.deposits.get(&1).unwrap();
        assert_eq!(*status, DepositStatus::Normal);
    }

    #[test]
    fn test_process_dispute_existing_deposit() {
        let mut engine = Engine::new();

        let deposit = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(10.0),
        };

        let dispute = DisputeTx {
            client_id: 1,
            tx_id: 1,
        };

        engine.process_deposit(deposit);
        engine.process_dispute(dispute);

        let (_, status) = engine.deposits.get(&1).unwrap();
        assert_eq!(*status, DepositStatus::UnderDispute);

        let client = engine.clients.get(&1).unwrap();
        assert_eq!(client.available, dec!(0));
        assert_eq!(client.total, dec!(10.0));
        assert_eq!(client.held, dec!(10.0));
    }

    #[test]
    fn test_process_dispute_existing_deposit_already_under_dispute() {
        let mut engine = Engine::new();

        let deposit = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(10.0),
        };

        let dispute1 = DisputeTx {
            client_id: 1,
            tx_id: 1,
        };

        let dispute2 = DisputeTx {
            client_id: 1,
            tx_id: 1,
        };

        engine.process_deposit(deposit);
        engine.process_dispute(dispute1);
        engine.process_dispute(dispute2);

        let (_, status) = engine.deposits.get(&1).unwrap();
        assert_eq!(*status, DepositStatus::UnderDispute);
        let client = engine.clients.get(&1).unwrap();
        assert_eq!(client.available, dec!(0));
        assert_eq!(client.total, dec!(10.0));
        assert_eq!(client.held, dec!(10.0));
    }

    #[test]
    fn test_process_multiple_disputes_same_client() {
        let mut engine = Engine::new();

        let deposit1 = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(10.0),
        };

        let deposit2 = DepositTx {
            client_id: 1,
            tx_id: 2,
            amount: dec!(20.0),
        };

        let dispute1 = DisputeTx {
            client_id: 1,
            tx_id: 1,
        };

        let dispute2 = DisputeTx {
            client_id: 1,
            tx_id: 2,
        };

        engine.process_deposit(deposit1);
        engine.process_deposit(deposit2);
        engine.process_dispute(dispute1);
        engine.process_dispute(dispute2);

        let (_, status) = engine.deposits.get(&1).unwrap();
        assert_eq!(*status, DepositStatus::UnderDispute);
        let (_, status) = engine.deposits.get(&2).unwrap();
        assert_eq!(*status, DepositStatus::UnderDispute);

        let client = engine.clients.get(&1).unwrap();
        assert_eq!(client.available, dec!(0));
        assert_eq!(client.total, dec!(30.0));
        assert_eq!(client.held, dec!(30.0));
    }

    #[test]
    fn test_process_dispute_wrong_client() {
        let mut engine = Engine::new();

        let deposit = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(100.0),
        };
        engine.process_deposit(deposit);

        let dispute = DisputeTx {
            client_id: 2,
            tx_id: 1,
        };
        engine.process_dispute(dispute);

        let (_, status) = engine.deposits.get(&1).unwrap();
        assert_eq!(*status, DepositStatus::Normal);

        let client1 = engine.clients.get(&1).unwrap();
        assert_eq!(client1.available, dec!(100.0));
    }

    #[test]
    fn test_process_deposit_into_withdrawal_into_dispute() {
        let mut engine = Engine::new();

        let deposit = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(10.0),
        };

        let withdrawal = WithdrawalTx {
            client_id: 1,
            tx_id: 2,
            amount: dec!(10.0),
        };

        let dispute = DisputeTx {
            client_id: 1,
            tx_id: 1,
        };

        engine.process_deposit(deposit);
        engine.process_withdrawal(withdrawal);
        engine.process_dispute(dispute);

        let (_, status) = engine.deposits.get(&1).unwrap();
        assert_eq!(*status, DepositStatus::UnderDispute);

        let client = engine.clients.get(&1).unwrap();
        assert_eq!(client.available, dec!(-10.0));
        assert_eq!(client.total, dec!(0));
        assert_eq!(client.held, dec!(10.0));
    }

    #[test]
    fn test_process_resolve_deposit_not_under_dispute() {
        let mut engine = Engine::new();

        let deposit = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(10.0),
        };

        let resolve = ResolveTx {
            client_id: 1,
            tx_id: 1,
        };

        engine.process_deposit(deposit);
        engine.process_resolve(resolve);

        let (_, status) = engine.deposits.get(&1).unwrap();
        assert_eq!(*status, DepositStatus::Normal);

        let client = engine.clients.get(&1).unwrap();
        assert_eq!(client.available, dec!(10.0));
        assert_eq!(client.total, dec!(10.0));
        assert_eq!(client.held, dec!(0));
    }

    #[test]
    fn test_process_resolve_deposit_under_dispute() {
        let mut engine = Engine::new();

        let deposit = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(20.0),
        };

        let dispute = DisputeTx {
            client_id: 1,
            tx_id: 1,
        };

        let resolve = ResolveTx {
            client_id: 1,
            tx_id: 1,
        };

        engine.process_deposit(deposit);
        engine.process_dispute(dispute);
        engine.process_resolve(resolve);

        let (_, status) = engine.deposits.get(&1).unwrap();
        assert_eq!(*status, DepositStatus::Resolved);

        let client = engine.clients.get(&1).unwrap();
        assert_eq!(client.available, dec!(20.0));
        assert_eq!(client.total, dec!(20.0));
        assert_eq!(client.held, dec!(0));
        assert!(!client.locked);
    }

    #[test]
    fn test_process_resolve_deposit_resolved() {
        let mut engine = Engine::new();

        let deposit = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(20.0),
        };

        let dispute = DisputeTx {
            client_id: 1,
            tx_id: 1,
        };

        let resolve1 = ResolveTx {
            client_id: 1,
            tx_id: 1,
        };

        let resolve2 = ResolveTx {
            client_id: 1,
            tx_id: 1,
        };

        engine.process_deposit(deposit);
        engine.process_dispute(dispute);
        engine.process_resolve(resolve1);
        engine.process_resolve(resolve2);

        let (_, status) = engine.deposits.get(&1).unwrap();
        assert_eq!(*status, DepositStatus::Resolved);

        let client = engine.clients.get(&1).unwrap();
        assert_eq!(client.available, dec!(20.0));
        assert_eq!(client.total, dec!(20.0));
        assert_eq!(client.held, dec!(0));
    }

    #[test]
    fn test_process_resolve_wrong_client() {
        let mut engine = Engine::new();

        let deposit = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(100.0),
        };
        let dispute = DisputeTx {
            client_id: 1,
            tx_id: 1,
        };

        engine.process_deposit(deposit);
        engine.process_dispute(dispute);

        let resolve = ResolveTx {
            client_id: 2,
            tx_id: 1,
        };
        engine.process_resolve(resolve);

        let (_, status) = engine.deposits.get(&1).unwrap();
        assert_eq!(*status, DepositStatus::UnderDispute);
    }

    #[test]
    fn test_process_dispute_after_resolve() {
        let mut engine = Engine::new();

        let deposit = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(100.0),
        };
        let dispute1 = DisputeTx {
            client_id: 1,
            tx_id: 1,
        };
        let resolve = ResolveTx {
            client_id: 1,
            tx_id: 1,
        };
        let dispute2 = DisputeTx {
            client_id: 1,
            tx_id: 1,
        };

        engine.process_deposit(deposit);
        engine.process_dispute(dispute1);
        engine.process_resolve(resolve);
        engine.process_dispute(dispute2);

        let (_, status) = engine.deposits.get(&1).unwrap();
        assert_eq!(*status, DepositStatus::Resolved);

        let client = engine.clients.get(&1).unwrap();
        assert_eq!(client.available, dec!(100.0));
        assert_eq!(client.held, dec!(0));
    }

    #[test]
    fn test_process_chargeback_deposit_not_under_dispute() {
        let mut engine = Engine::new();

        let deposit = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(10.0),
        };

        let chargeback = ChargebackTx {
            client_id: 1,
            tx_id: 1,
        };

        engine.process_deposit(deposit);
        engine.process_chargeback(chargeback);

        let (_, status) = engine.deposits.get(&1).unwrap();
        assert_eq!(*status, DepositStatus::Normal);

        let client = engine.clients.get(&1).unwrap();
        assert_eq!(client.available, dec!(10.0));
        assert_eq!(client.total, dec!(10.0));
        assert_eq!(client.held, dec!(0));
        assert!(!client.locked);
    }

    #[test]
    fn test_process_chargeback_deposit_under_dispute() {
        let mut engine = Engine::new();

        let deposit = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(20.0),
        };

        let dispute = DisputeTx {
            client_id: 1,
            tx_id: 1,
        };

        let chargeback = ChargebackTx {
            client_id: 1,
            tx_id: 1,
        };

        engine.process_deposit(deposit);
        engine.process_dispute(dispute);
        engine.process_chargeback(chargeback);

        let (_, status) = engine.deposits.get(&1).unwrap();
        assert_eq!(*status, DepositStatus::ChargedBack);

        let client = engine.clients.get(&1).unwrap();
        assert_eq!(client.available, dec!(0));
        assert_eq!(client.total, dec!(0));
        assert_eq!(client.held, dec!(0));
        assert!(client.locked);
    }

    #[test]
    fn test_process_chargeback_deposit_charged_back() {
        let mut engine = Engine::new();

        let deposit = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(20.0),
        };

        let dispute = DisputeTx {
            client_id: 1,
            tx_id: 1,
        };

        let chargeback1 = ChargebackTx {
            client_id: 1,
            tx_id: 1,
        };

        let chargeback2 = ChargebackTx {
            client_id: 1,
            tx_id: 1,
        };

        engine.process_deposit(deposit);
        engine.process_dispute(dispute);
        engine.process_chargeback(chargeback1);
        engine.process_chargeback(chargeback2);

        let (_, status) = engine.deposits.get(&1).unwrap();
        assert_eq!(*status, DepositStatus::ChargedBack);

        let client = engine.clients.get(&1).unwrap();
        assert_eq!(client.available, dec!(0));
        assert_eq!(client.total, dec!(0));
        assert_eq!(client.held, dec!(0));
        assert!(client.locked);
    }

    #[test]
    fn test_process_deposit_on_locked_account() {
        let mut engine = Engine::new();

        let deposit1 = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(100.0),
        };
        let dispute = DisputeTx {
            client_id: 1,
            tx_id: 1,
        };
        let chargeback = ChargebackTx {
            client_id: 1,
            tx_id: 1,
        };

        engine.process_deposit(deposit1);
        engine.process_dispute(dispute);
        engine.process_chargeback(chargeback);

        let deposit2 = DepositTx {
            client_id: 1,
            tx_id: 2,
            amount: dec!(50.0),
        };
        engine.process_deposit(deposit2);

        let client = engine.clients.get(&1).unwrap();
        assert!(client.locked);
        assert_eq!(client.total, dec!(0));
        assert!(!engine.deposits.contains_key(&2));
    }

    #[test]
    fn test_process_withdrawal_on_locked_account() {
        let mut engine = Engine::new();

        let deposit1 = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(100.0),
        };
        let deposit2 = DepositTx {
            client_id: 1,
            tx_id: 2,
            amount: dec!(50.0),
        };
        let dispute = DisputeTx {
            client_id: 1,
            tx_id: 1,
        };
        let chargeback = ChargebackTx {
            client_id: 1,
            tx_id: 1,
        };

        engine.process_deposit(deposit1);
        engine.process_deposit(deposit2);
        engine.process_dispute(dispute);
        engine.process_chargeback(chargeback);

        let withdrawal = WithdrawalTx {
            client_id: 1,
            tx_id: 3,
            amount: dec!(25.0),
        };
        engine.process_withdrawal(withdrawal);

        let client = engine.clients.get(&1).unwrap();
        assert_eq!(client.available, dec!(50.0));
    }

    #[test]
    fn test_chargeback_locks_account_but_allows_resolving_other_disputes() {
        let mut engine = Engine::new();

        let deposit1 = DepositTx {
            client_id: 1,
            tx_id: 1,
            amount: dec!(100.0),
        };

        let deposit2 = DepositTx {
            client_id: 1,
            tx_id: 2,
            amount: dec!(50.0),
        };

        let dispute1 = DisputeTx {
            client_id: 1,
            tx_id: 1,
        };

        let dispute2 = DisputeTx {
            client_id: 1,
            tx_id: 2,
        };

        let chargeback1 = ChargebackTx {
            client_id: 1,
            tx_id: 1,
        };

        let resolve2 = ResolveTx {
            client_id: 1,
            tx_id: 2,
        };

        engine.process_deposit(deposit1);
        engine.process_deposit(deposit2);
        engine.process_dispute(dispute1);
        engine.process_dispute(dispute2);

        let client = engine.clients.get(&1).unwrap();
        assert_eq!(client.available, dec!(0));
        assert_eq!(client.held, dec!(150.0));
        assert_eq!(client.total, dec!(150.0));

        engine.process_chargeback(chargeback1);

        let client = engine.clients.get(&1).unwrap();
        assert!(client.locked);
        assert_eq!(client.available, dec!(0));
        assert_eq!(client.held, dec!(50.0));
        assert_eq!(client.total, dec!(50.0));

        engine.process_resolve(resolve2);

        let client = engine.clients.get(&1).unwrap();
        assert!(client.locked);
        assert_eq!(client.available, dec!(50.0));
        assert_eq!(client.held, dec!(0));
        assert_eq!(client.total, dec!(50.0));

        let (_, status1) = engine.deposits.get(&1).unwrap();
        assert_eq!(*status1, DepositStatus::ChargedBack);

        let (_, status2) = engine.deposits.get(&2).unwrap();
        assert_eq!(*status2, DepositStatus::Resolved);
    }

    #[test]
    fn test_process_deposit_rejected_after_chargeback_locks_account() {
        let mut engine = Engine::new();

        let deposit1 = DepositTx {
            client_id: 2,
            tx_id: 2,
            amount: dec!(2000.75),
        };

        let withdrawal = WithdrawalTx {
            client_id: 2,
            tx_id: 6,
            amount: dec!(500.0),
        };

        let deposit2 = DepositTx {
            client_id: 2,
            tx_id: 10,
            amount: dec!(1500.0),
        };

        let dispute = DisputeTx {
            client_id: 2,
            tx_id: 2,
        };

        let chargeback = ChargebackTx {
            client_id: 2,
            tx_id: 2,
        };

        let deposit3 = DepositTx {
            client_id: 2,
            tx_id: 22,
            amount: dec!(500.0),
        };

        engine.process_deposit(deposit1);
        engine.process_withdrawal(withdrawal);
        engine.process_deposit(deposit2);

        let client = engine.clients().get(&2).unwrap();
        assert_eq!(client.available, dec!(3000.75));
        assert_eq!(client.total, dec!(3000.75));

        engine.process_dispute(dispute);

        let client = engine.clients().get(&2).unwrap();
        assert_eq!(client.available, dec!(1000.0));
        assert_eq!(client.held, dec!(2000.75));
        assert_eq!(client.total, dec!(3000.75));

        engine.process_chargeback(chargeback);

        let client = engine.clients().get(&2).unwrap();
        assert_eq!(client.available, dec!(1000.0));
        assert_eq!(client.held, dec!(0));
        assert_eq!(client.total, dec!(1000.0));
        assert!(client.locked);

        engine.process_deposit(deposit3);

        let client = engine.clients().get(&2).unwrap();
        assert_eq!(client.available, dec!(1000.0));
        assert_eq!(client.held, dec!(0));
        assert_eq!(client.total, dec!(1000.0));
        assert!(client.locked);
    }

    #[test]
    fn test_end_to_end_csv_processing() {
        // Note: This duplicates CSV processing logic from main.rs
        // Could be extracted to Engine::process_csv() to reduce duplication
        const TEST_CSV: &str = "\
type,client,tx,amount
deposit,1,1,100.0
deposit,2,2,200.0
deposit,1,3,50.0
withdrawal,1,4,30.0
dispute,1,1
resolve,1,1
deposit,2,5,100.0
dispute,2,2
chargeback,2,2
deposit,2,6,50.0";

        let mut input_file = NamedTempFile::new().unwrap();
        write!(input_file, "{}", TEST_CSV).unwrap();
        input_file.flush().unwrap();

        let mut rdr = csv::ReaderBuilder::new()
            .trim(csv::Trim::All)
            .flexible(true)
            .from_path(input_file.path())
            .unwrap();

        let mut engine = Engine::new();

        for result in rdr.deserialize() {
            let record: CsvRow = match result {
                Ok(r) => r,
                Err(_) => continue,
            };

            let tx = match Tx::try_from(record) {
                Ok(t) => t,
                Err(_) => continue,
            };

            engine.process_tx(tx);
        }

        let client1 = engine.clients().get(&1).unwrap();
        assert_eq!(client1.available, dec!(120.0));
        assert_eq!(client1.held, dec!(0));
        assert_eq!(client1.total, dec!(120.0));
        assert!(!client1.locked);

        let client2 = engine.clients().get(&2).unwrap();
        assert_eq!(client2.available, dec!(100.0));
        assert_eq!(client2.held, dec!(0));
        assert_eq!(client2.total, dec!(100.0));
        assert!(client2.locked);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;
    use rust_decimal::Decimal;

    fn arb_transaction() -> impl Strategy<Value = Tx> {
        prop_oneof![
            (1u16..100, 1u32..10000, 0i64..100000).prop_map(|(client, tx, amount)| {
                Tx::Deposit(DepositTx {
                    client_id: client,
                    tx_id: tx,
                    amount: Decimal::new(amount, 4), // amount/10000 for 4 decimals
                })
            }),
            (1u16..100, 1u32..10000, 0i64..100000).prop_map(|(client, tx, amount)| {
                Tx::Withdrawal(WithdrawalTx {
                    client_id: client,
                    tx_id: tx,
                    amount: Decimal::new(amount, 4),
                })
            }),
            (1u16..100, 1u32..10000).prop_map(|(client, tx)| {
                Tx::Dispute(DisputeTx {
                    client_id: client,
                    tx_id: tx,
                })
            }),
            (1u16..100, 1u32..10000).prop_map(|(client, tx)| {
                Tx::Resolve(ResolveTx {
                    client_id: client,
                    tx_id: tx,
                })
            }),
            (1u16..100, 1u32..10000).prop_map(|(client, tx)| {
                Tx::Chargeback(ChargebackTx {
                    client_id: client,
                    tx_id: tx,
                })
            }),
        ]
    }

    proptest! {
        #[test]
        fn test_engine_never_panics(txs in prop::collection::vec(arb_transaction(), 0..1000)) {
            let mut engine = Engine::new();

            // Process all transactions - should never panic
            for tx in txs {
                engine.process_tx(tx);
            }

            // Invariant checks
            for (_, client) in engine.clients.iter() {
                prop_assert_eq!(client.total, client.available + client.held);
                prop_assert!(client.held >= Decimal::ZERO);
            }
        }

        #[test]
        fn test_invariants_hold(txs in prop::collection::vec(arb_transaction(), 0..500)) {
            let mut engine = Engine::new();

            for tx in txs {
                engine.process_tx(tx);

                // After every transaction, check invariants
                for (_, client) in engine.clients.iter() {
                    prop_assert_eq!(
                        client.available + client.held,
                        client.total,
                        "Invariant violated: available + held != total"
                    );
                    prop_assert!(
                        client.held >= Decimal::ZERO,
                        "Invariant violated: held is negative"
                    );
                }
            }
        }
    }
}
