//! Defines the core data structures and types used by the payment engine.

use std::collections::{HashMap, hash_map::Entry};

use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde::Deserialize;

/// Represents various errors that can occur during transaction processing.
#[derive(Debug)]
pub enum Errors {
    AccountNotFound,
    InsufficientFunds,
    AccountLocked,
    InvalidAmount,
    TransactionNotFound,
    TransactionAlreadyDisputed,
    TransactionStateNotDisputed,
    TransactionClientMismatch,
    DuplicateTransaction,
    CalculationOverflow,
}

/// Represents a raw transaction deserialized directly from the CSV input.
#[derive(Deserialize, Debug)]
pub struct RawTransaction {
    pub client: u16,
    pub tx: u32,
    pub amount: Option<Decimal>,
    #[serde(rename = "type")]
    pub operation: Operation,
}

/// Defines the types of operations a transaction can represent.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

/// The central data structure holding all state for the payment engine.
pub struct Record {
    pub accounts: AccountStore,
    pub transactions: TransactionStore,
}

impl Record {
    /// Creates a new, empty `Record`.
    pub fn new() -> Self {
        Self {
            accounts: AccountStore {
                storage: HashMap::new(),
            },
            transactions: TransactionStore {
                storage: HashMap::new(),
            },
        }
    }
}

/// Represents a client's account, holding available, held, and locked statuses.
#[derive(Debug)]
pub struct Account {
    pub available: i64,
    pub held: i64,
    pub locked: bool,
}

impl Account {
    /// Calculates the total funds of the account (available + held).
    pub fn total(&self) -> i64 {
        self.available + self.held
    }
}

/// Encapsulates the storage and retrieval of client accounts.
pub struct AccountStore {
    pub storage: HashMap<u16, Account>,
}

impl AccountStore {
    /// Gets an immutable reference to a client's account.
    pub fn get(&self, client: u16) -> Result<&Account, Errors> {
        self.storage.get(&client).ok_or(Errors::AccountNotFound)
    }

    /// Gets a mutable reference to a client's account.
    pub fn get_mut(&mut self, client: u16) -> Result<&mut Account, Errors> {
        self.storage.get_mut(&client).ok_or(Errors::AccountNotFound)
    }

    /// Retrieves an existing account mutably, or creates a new one if it does not exist.
    pub fn get_or_create(&mut self, client: u16) -> Result<&mut Account, Errors> {
        let account = self.storage.entry(client).or_insert_with(|| Account {
            available: 0,
            held: 0,
            locked: false,
        });
        if account.locked {
            return Err(Errors::AccountLocked);
        }
        Ok(account)
    }

    /// Ensures that an account exists and is not locked.
    pub fn account_not_locked(&self, client: u16) -> Result<(), Errors> {
        let account = self.get(client)?;
        if account.locked {
            return Err(Errors::AccountLocked);
        }
        Ok(())
    }
}

/// Represents a transaction stored within the engine's memory.
#[derive(Debug)]
pub struct TransactionRecord {
    pub client: u16,
    pub tx: u32,
    pub amount: i64,
    pub state: DisputeStatus,
}

/// Represents the current dispute state of a transaction.
#[derive(Debug, PartialEq)]
pub enum DisputeStatus {
    Undisputed,
    Disputed,
    Resolved,
    Chargeback,
}

/// Encapsulates the storage and retrieval of transaction records.
pub struct TransactionStore {
    storage: HashMap<u32, TransactionRecord>,
}

impl TransactionStore {
    /// Gets an immutable reference to a transaction.
    pub fn get(&self, tx: u32) -> Result<&TransactionRecord, Errors> {
        self.storage.get(&tx).ok_or(Errors::TransactionNotFound)
    }

    /// Gets a mutable reference to a transaction.
    pub fn get_mut(&mut self, tx: u32) -> Result<&mut TransactionRecord, Errors> {
        self.storage.get_mut(&tx).ok_or(Errors::TransactionNotFound)
    }

    /// Inserts a new transaction record into the store.
    ///
    /// Returns `Err(DuplicateTransaction)` if a record with the same tx ID already exists.
    pub fn insert(&mut self, record: TransactionRecord) -> Result<(), Errors> {
        match self.storage.entry(record.tx) {
            Entry::Occupied(_) => Err(Errors::DuplicateTransaction),
            Entry::Vacant(e) => {
                e.insert(record);
                Ok(())
            }
        }
    }
}

/// The scaling factor used to convert Decimals to integers. (10,000 for 4 decimal places)
pub const MULTIPLIER: i64 = 10_000;

/// Converts a standard `Decimal` to a scaled fixed-point integer representation.
pub fn to_fixed(value: Decimal) -> Result<i64, Errors> {
    if value.is_sign_negative() {
        return Err(Errors::InvalidAmount);
    }
    value
        .checked_mul(Decimal::from(MULTIPLIER))
        .ok_or(Errors::InvalidAmount)?
        .trunc()
        .to_i64()
        .ok_or(Errors::InvalidAmount)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_account_total() {
        let acc = Account {
            available: 100,
            held: 50,
            locked: false,
        };
        assert_eq!(acc.total(), 150);
    }

    #[test]
    fn test_to_fixed() {
        assert_eq!(
            to_fixed(Decimal::from_str("10.5").unwrap()).unwrap(),
            105000
        );
        assert_eq!(to_fixed(Decimal::from_str("0.0001").unwrap()).unwrap(), 1);
        assert_eq!(to_fixed(Decimal::from_str("0").unwrap()).unwrap(), 0);

        // Truncation happens if more than 4 decimals
        assert_eq!(to_fixed(Decimal::from_str("0.00009").unwrap()).unwrap(), 0);

        // Negatives are an error
        assert!(matches!(
            to_fixed(Decimal::from_str("-10.0").unwrap()),
            Err(Errors::InvalidAmount)
        ));
    }

    #[test]
    fn test_account_store() {
        let mut store = AccountStore {
            storage: HashMap::new(),
        };

        // Get non-existent
        assert!(matches!(store.get(1), Err(Errors::AccountNotFound)));

        // Create
        store.get_or_create(1).unwrap().available += 100;
        assert_eq!(store.get(1).unwrap().available, 100);

        // Mutate
        store.get_mut(1).unwrap().available += 50;
        assert_eq!(store.get(1).unwrap().available, 150);

        // Lock checks
        store.get_mut(1).unwrap().locked = true;
        assert!(matches!(
            store.account_not_locked(1),
            Err(Errors::AccountLocked)
        ));
        assert!(matches!(store.get_or_create(1), Err(Errors::AccountLocked)));
    }

    #[test]
    fn test_transaction_store() {
        let mut store = TransactionStore {
            storage: HashMap::new(),
        };

        assert!(matches!(store.get(1), Err(Errors::TransactionNotFound)));

        let stored = store.insert(TransactionRecord {
            client: 1,
            tx: 1,
            amount: 500,
            state: DisputeStatus::Undisputed,
        });
        assert!(stored.is_ok());

        assert_eq!(store.get(1).unwrap().amount, 500);

        store.get_mut(1).unwrap().state = DisputeStatus::Disputed;
        assert_eq!(store.get(1).unwrap().state, DisputeStatus::Disputed);
    }
}
