//! Contains the core business logic for processing transactions.

use crate::types::{
    DisputeStatus, Errors, Operation, RawTransaction, Record, TransactionRecord, to_fixed,
};

/// The main engine responsible for applying transactions to client accounts.
pub struct PaymentsEngine {
    pub records: Record,
}

impl PaymentsEngine {
    /// Creates a new, empty `PaymentsEngine`.
    pub fn new() -> Self {
        Self {
            records: Record::new(),
        }
    }

    /// Processes a single raw transaction, updating account state accordingly.
    ///
    /// # Arguments
    ///
    /// * `raw` - The raw transaction data parsed from the CSV.
    ///
    /// # Errors
    ///
    /// Returns an error if the transaction is invalid, the account is locked, or funds are insufficient.
    pub fn process_transaction(&mut self, raw: &RawTransaction) -> Result<(), Errors> {
        if raw.operation == Operation::Withdrawal {
            self.records.accounts.account_not_locked(raw.client)?;
        }

        match raw.operation {
            Operation::Deposit => {
                let amount = raw.amount.ok_or(Errors::InvalidAmount).and_then(to_fixed)?;
                self.deposit(raw.client, raw.tx, amount)
            }
            Operation::Withdrawal => {
                let amount = raw.amount.ok_or(Errors::InvalidAmount).and_then(to_fixed)?;
                self.withdrawal(raw.client, amount)
            }
            Operation::Dispute => self.dispute(raw.client, raw.tx),
            Operation::Resolve => self.resolve(raw.client, raw.tx),
            Operation::Chargeback => self.chargeback(raw.client, raw.tx),
        }
    }

    /// Processes a deposit operation.
    fn deposit(&mut self, client: u16, tx: u32, amount: i64) -> Result<(), Errors> {
        let account = self.records.accounts.get_or_create(client)?;

        let _ = account
            .total()
            .checked_add(amount)
            .ok_or(Errors::CalculationOverflow)?;

        let new_available = account
            .available
            .checked_add(amount)
            .ok_or(Errors::CalculationOverflow)?;

        self.records.transactions.insert(TransactionRecord {
            client,
            tx,
            amount,
            state: DisputeStatus::Undisputed,
        })?;

        account.available = new_available;

        Ok(())
    }

    /// Processes a withdrawal operation.
    fn withdrawal(&mut self, client: u16, amount: i64) -> Result<(), Errors> {
        let account = self.records.accounts.get_mut(client)?;

        if account.available < amount {
            return Err(Errors::InsufficientFunds);
        }

        account.available -= amount;
        Ok(())
    }

    /// Processes a dispute operation.
    fn dispute(&mut self, client: u16, tx: u32) -> Result<(), Errors> {
        let account = self.records.accounts.get_mut(client)?;

        let transaction = self.records.transactions.get(tx)?;
        if client != transaction.client {
            return Err(Errors::TransactionClientMismatch);
        }
        if transaction.state != DisputeStatus::Undisputed {
            return Err(Errors::TransactionAlreadyDisputed);
        }
        let tx_amount = transaction.amount;

        self.records.transactions.get_mut(tx)?.state = DisputeStatus::Disputed;

        account.available -= tx_amount;
        account.held += tx_amount;

        Ok(())
    }

    /// Processes a resolve operation.
    fn resolve(&mut self, client: u16, tx: u32) -> Result<(), Errors> {
        let dispute_amount = self.validate_dispute(client, tx)?;
        let account = self.records.accounts.get_mut(client)?;

        self.records.transactions.get_mut(tx)?.state = DisputeStatus::Resolved;

        account.available += dispute_amount;
        account.held -= dispute_amount;
        Ok(())
    }

    /// Processes a chargeback operation.
    fn chargeback(&mut self, client: u16, tx: u32) -> Result<(), Errors> {
        let dispute_amount = self.validate_dispute(client, tx)?;
        let account = self.records.accounts.get_mut(client)?;

        self.records.transactions.get_mut(tx)?.state = DisputeStatus::Chargeback;

        account.held -= dispute_amount;
        account.locked = true;
        Ok(())
    }

    /// Validates a dispute-related transaction (resolve or chargeback).
    fn validate_dispute(&self, client: u16, tx: u32) -> Result<i64, Errors> {
        let transaction = self.records.transactions.get(tx)?;
        if client != transaction.client {
            return Err(Errors::TransactionClientMismatch);
        }
        if transaction.state != DisputeStatus::Disputed {
            return Err(Errors::TransactionStateNotDisputed);
        }
        Ok(transaction.amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn test_engine() -> PaymentsEngine {
        PaymentsEngine::new()
    }

    #[test]
    fn test_deposit_and_withdrawal() {
        let mut engine = test_engine();

        // Deposit
        engine
            .process_transaction(&RawTransaction {
                client: 1,
                tx: 1,
                amount: Some(Decimal::from_str("100.5").unwrap()),
                operation: Operation::Deposit,
            })
            .unwrap();

        let acc = engine.records.accounts.get(1).unwrap();
        assert_eq!(acc.available, 1005000);
        assert_eq!(acc.total(), 1005000);

        // Valid withdrawal
        engine
            .process_transaction(&RawTransaction {
                client: 1,
                tx: 2,
                amount: Some(Decimal::from_str("50.0").unwrap()),
                operation: Operation::Withdrawal,
            })
            .unwrap();

        let acc = engine.records.accounts.get(1).unwrap();
        assert_eq!(acc.available, 505000);
        assert_eq!(acc.total(), 505000);

        // Invalid withdrawal (insufficient funds)
        let res = engine.process_transaction(&RawTransaction {
            client: 1,
            tx: 3,
            amount: Some(Decimal::from_str("60.0").unwrap()),
            operation: Operation::Withdrawal,
        });
        assert!(matches!(res, Err(Errors::InsufficientFunds)));
    }

    #[test]
    fn test_dispute_and_resolve() {
        let mut engine = test_engine();
        engine
            .process_transaction(&RawTransaction {
                client: 1,
                tx: 1,
                amount: Some(Decimal::from_str("100.0").unwrap()),
                operation: Operation::Deposit,
            })
            .unwrap();

        // Dispute
        engine
            .process_transaction(&RawTransaction {
                client: 1,
                tx: 1,
                amount: None,
                operation: Operation::Dispute,
            })
            .unwrap();

        let acc = engine.records.accounts.get(1).unwrap();
        assert_eq!(acc.available, 0);
        assert_eq!(acc.held, 1000000);
        assert_eq!(acc.total(), 1000000);

        // Resolve
        engine
            .process_transaction(&RawTransaction {
                client: 1,
                tx: 1,
                amount: None,
                operation: Operation::Resolve,
            })
            .unwrap();

        let acc = engine.records.accounts.get(1).unwrap();
        assert_eq!(acc.available, 1000000);
        assert_eq!(acc.held, 0);
    }

    #[test]
    fn test_chargeback() {
        let mut engine = test_engine();
        engine
            .process_transaction(&RawTransaction {
                client: 1,
                tx: 1,
                amount: Some(Decimal::from_str("100.0").unwrap()),
                operation: Operation::Deposit,
            })
            .unwrap();

        engine
            .process_transaction(&RawTransaction {
                client: 1,
                tx: 1,
                amount: None,
                operation: Operation::Dispute,
            })
            .unwrap();

        // Chargeback
        engine
            .process_transaction(&RawTransaction {
                client: 1,
                tx: 1,
                amount: None,
                operation: Operation::Chargeback,
            })
            .unwrap();

        let acc = engine.records.accounts.get(1).unwrap();
        assert_eq!(acc.available, 0);
        assert_eq!(acc.held, 0);
        assert_eq!(acc.total(), 0);
        assert!(acc.locked);

        // Try to withdraw from locked account
        let res = engine.process_transaction(&RawTransaction {
            client: 1,
            tx: 2,
            amount: Some(Decimal::from_str("10.0").unwrap()),
            operation: Operation::Withdrawal,
        });
        assert!(matches!(res, Err(Errors::AccountLocked)));
    }

    #[test]
    fn test_invalid_disputes() {
        let mut engine = test_engine();
        engine
            .process_transaction(&RawTransaction {
                client: 1,
                tx: 1,
                amount: Some(Decimal::from_str("100.0").unwrap()),
                operation: Operation::Deposit,
            })
            .unwrap();

        // Dispute wrong client
        let res = engine.process_transaction(&RawTransaction {
            client: 2,
            tx: 1,
            amount: None,
            operation: Operation::Dispute,
        });
        // Returns AccountNotFound because client 2 doesn't exist yet,
        // or TransactionClientMismatch if we get past that.
        // Actually, for Dispute, it checks `account_not_locked(raw.client)` first.
        // Since client 2 doesn't exist, it fails with AccountNotFound.
        assert!(matches!(res, Err(Errors::AccountNotFound)));

        // Create client 2
        engine
            .process_transaction(&RawTransaction {
                client: 2,
                tx: 2,
                amount: Some(Decimal::from_str("50.0").unwrap()),
                operation: Operation::Deposit,
            })
            .unwrap();

        // Dispute wrong client but account exists
        let res = engine.process_transaction(&RawTransaction {
            client: 2,
            tx: 1,
            amount: None,
            operation: Operation::Dispute,
        });
        assert!(matches!(res, Err(Errors::TransactionClientMismatch)));

        // Resolve undisputed
        let res = engine.process_transaction(&RawTransaction {
            client: 1,
            tx: 1,
            amount: None,
            operation: Operation::Resolve,
        });
        assert!(matches!(res, Err(Errors::TransactionStateNotDisputed)));
    }
}
