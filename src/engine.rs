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

struct Engine {
    clients: HashMap<ClientId, Client>,
    // TODO: we could make DepositStatus an Option to reduce the mem footprint
    deposits: HashMap<TxId, (DepositTx, DepositStatus)>,
}

impl Engine {
    pub fn new() -> Self {
        Engine {
            clients: HashMap::new(),
            deposits: HashMap::new(),
        }
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
            return;
        }

        // Update the available
        client.available += deposit_tx.amount;
        // Update the total
        client.total += deposit_tx.amount;

        // Spec claims that the ids are unique, but just to be sure
        // TODO: if we assume tx_ids can have duplicates then we would have to store
        // all of the txs
        self.deposits
            .entry(deposit_tx.tx_id)
            .or_insert((deposit_tx, DepositStatus::Normal));
    }

    fn process_withdrawal(&mut self, withdrawal_tx: WithdrawalTx) {
        let Some(client) = self.clients.get_mut(&withdrawal_tx.client_id) else {
            return; // Client doesn't exist
        };

        if client.locked {
            return;
        }

        if client.available < withdrawal_tx.amount {
            return; // Insufficient funds
        }

        client.available -= withdrawal_tx.amount;
        client.total -= withdrawal_tx.amount;
    }

    fn process_dispute(&mut self, dispute_tx: DisputeTx) {
        let Some(client) = self.clients.get_mut(&dispute_tx.client_id) else {
            return;
        };

        let Some((deposit_tx, deposit_status)) = self.deposits.get_mut(&dispute_tx.tx_id) else {
            return;
        };

        // Verify client matches
        if dispute_tx.client_id != deposit_tx.client_id {
            return;
        }

        // Only dispute if in Normal status
        if *deposit_status != DepositStatus::Normal {
            return;
        }

        // All checks passed, process the dispute
        *deposit_status = DepositStatus::UnderDispute;
        client.available -= deposit_tx.amount;
        client.held += deposit_tx.amount;
    }

    fn process_resolve(&mut self, resolve_tx: ResolveTx) {
        let Some(client) = self.clients.get_mut(&resolve_tx.client_id) else {
            return;
        };

        let Some((deposit_tx, deposit_status)) = self.deposits.get_mut(&resolve_tx.tx_id) else {
            return;
        };

        if resolve_tx.client_id != deposit_tx.client_id {
            return;
        }

        if *deposit_status != DepositStatus::UnderDispute {
            return;
        }

        *deposit_status = DepositStatus::Resolved;
        client.available += deposit_tx.amount;
        client.held -= deposit_tx.amount;
    }

    fn process_chargeback(&mut self, chargeback_tx: ChargebackTx) {
        let Some(client) = self.clients.get_mut(&chargeback_tx.client_id) else {
            return;
        };

        let Some((deposit_tx, deposit_status)) = self.deposits.get_mut(&chargeback_tx.tx_id) else {
            return;
        };

        if chargeback_tx.client_id != deposit_tx.client_id {
            return;
        }

        if *deposit_status != DepositStatus::UnderDispute {
            return;
        }

        *deposit_status = DepositStatus::ChargedBack;
        client.total -= deposit_tx.amount;
        client.held -= deposit_tx.amount;
        client.locked = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

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

        // Verify deposit is stored
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
        // Check if dispute did not add new entry
        assert!(!engine.deposits.contains_key(&2));
        // Check if dispute did not update the status of existing deposit
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

        // Check if dispute updated the status
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

        // Check if second despute was not applied
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
    fn test_deposit_on_locked_account() {
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
    fn test_withdrawal_on_locked_account() {
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
}
