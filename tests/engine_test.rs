//! Integration tests for the payment engine.
//!
//! Each test feeds CSV input through `run_engine`, then collects the output
//! from `print_accounts` and asserts against the expected account state.

use std::collections::BTreeMap;

use payment_engine::engine::PaymentsEngine;
use payment_engine::{print_accounts, run_engine};

#[derive(Debug, PartialEq)]
struct ExpectedAccount {
    available: i64,
    held: i64,
    total: i64,
    locked: bool,
}

/// Helper: runs a CSV string through the engine and returns the final account states
/// as a sorted map of client_id -> ExpectedAccount.
fn run_scenario(csv: &str) -> BTreeMap<u16, ExpectedAccount> {
    let mut engine = PaymentsEngine::new();
    run_engine(csv.as_bytes(), &mut engine).expect("run_engine failed");

    engine
        .records
        .accounts
        .storage
        .iter()
        .map(|(&client, acc)| {
            (
                client,
                ExpectedAccount {
                    available: acc.available,
                    held: acc.held,
                    total: acc.total(),
                    locked: acc.locked,
                },
            )
        })
        .collect()
}

/// Helper: runs a CSV string through the engine, writes output via `print_accounts`,
/// and returns the raw CSV output string.
fn run_scenario_output(csv: &str) -> String {
    let mut engine = PaymentsEngine::new();
    run_engine(csv.as_bytes(), &mut engine).expect("run_engine failed");

    let mut out = Vec::new();
    print_accounts(&mut out, &engine.records.accounts.storage).expect("print_accounts failed");
    String::from_utf8(out).expect("output is not valid UTF-8")
}

#[test]
fn basic_deposits_and_withdrawals() {
    let csv = "\
        type,client,tx,amount
        deposit,1,1,100.50
        deposit,2,2,200.00
        withdrawal,1,3,50.25
        deposit,1,4,10.00
        withdrawal,2,5,100.00
    ";

    let accounts = run_scenario(csv);

    assert_eq!(
        accounts[&1],
        ExpectedAccount {
            available: 602500,
            held: 0,
            total: 602500,
            locked: false,
        }
    );
    assert_eq!(
        accounts[&2],
        ExpectedAccount {
            available: 1000000,
            held: 0,
            total: 1000000,
            locked: false,
        }
    );
}

#[test]
fn insufficient_funds_for_withdrawal() {
    let csv = "\
        type,client,tx,amount
        deposit,1,1,50.00
        withdrawal,1,2,100.00
        deposit,1,3,20.00
        withdrawal,1,4,70.01
    ";

    let accounts = run_scenario(csv);

    assert_eq!(
        accounts[&1],
        ExpectedAccount {
            available: 700000,
            held: 0,
            total: 700000,
            locked: false,
        }
    );
}

#[test]
fn dispute_set_funds_in_held() {
    let csv = "\
        type,client,tx,amount
        deposit,1,4,500.00
        withdrawal,1,2,100.00
        dispute,1,4,
    ";

    let accounts = run_scenario(csv);

    assert_eq!(
        accounts[&1],
        ExpectedAccount {
            available: -1000000,
            held: 5000000,
            total: 4000000,
            locked: false,
        }
    );
}

#[test]
fn dispute_and_resolve() {
    let csv = "\
        type,client,tx,amount
        deposit,1,1,500.00
        withdrawal,1,2,100.00
        dispute,1,1,
        resolve,1,1,
    ";

    let accounts = run_scenario(csv);

    assert_eq!(
        accounts[&1],
        ExpectedAccount {
            available: 4000000,
            held: 0,
            total: 4000000,
            locked: false,
        }
    );
}

#[test]
fn cannot_overflow_account_balances_on_deposit() {
    let max_amount = i64::MAX / 10000;
    let csv = format!(
        "\
        type,client,tx,amount
        deposit,1,1,{}
        deposit,1,2, 1000000000000
    ",
        max_amount
    );

    let accounts = run_scenario(&csv);

    assert_eq!(
        accounts[&1],
        ExpectedAccount {
            available: max_amount * 10000,
            held: 0,
            total: max_amount * 10000,
            locked: false,
        }
    );
}

#[test]
fn cannot_overflow_account_balances_on_disputes() {
    let max_amount = i64::MAX / 10000;
    let csv = format!(
        "\
        type,client,tx,amount
        deposit,1,1,{}
        dispute,1,1,
        deposit,1,2, 1000000000000
    ",
        max_amount
    );

    let accounts = run_scenario(&csv);

    assert_eq!(
        accounts[&1],
        ExpectedAccount {
            available: 0,
            held: max_amount * 10000,
            total: max_amount * 10000,
            locked: false,
        }
    );
}

#[test]
fn dispute_and_chargeback_locks_account() {
    let csv = "\
        type,client,tx,amount
        deposit,1,1,150.00
        deposit,1,2,50.00
        dispute,1,1,
        chargeback,1,1,
    ";

    let accounts = run_scenario(csv);

    assert_eq!(
        accounts[&1],
        ExpectedAccount {
            available: 500000,
            held: 0,
            total: 500000,
            locked: true,
        }
    );
}

#[test]
fn post_lock_rejection() {
    let csv = "\
        type,client,tx,amount
        deposit,1,1,100.00
        deposit,1,2,200.00
        dispute,1,1,
        chargeback,1,1,
        deposit,1,3,500.00
        withdrawal,1,4,50.00
    ";

    let accounts = run_scenario(csv);

    assert_eq!(
        accounts[&1],
        ExpectedAccount {
            available: 2000000,
            held: 0,
            total: 2000000,
            locked: true,
        }
    );
}

#[test]
fn invalid_state_transitions() {
    let csv = "\
        type,client,tx,amount
        deposit,1,1,100.00
        resolve,1,1,
        chargeback,1,1,
        dispute,1,999,
        dispute,1,1,
        dispute,1,1,
        resolve,1,1,
        resolve,1,1,
    ";

    let accounts = run_scenario(csv);

    assert_eq!(
        accounts[&1],
        ExpectedAccount {
            available: 1000000,
            held: 0,
            total: 1000000,
            locked: false,
        }
    );
}

#[test]
fn whitespace_tolerance() {
    let csv = "\
        type, client, tx, amount
        deposit , 1 , 1 , 100.00 
        deposit, 1, 2,  50.00
        withdrawal,1 ,3, 25.00
    ";

    let accounts = run_scenario(csv);

    assert_eq!(
        accounts[&1],
        ExpectedAccount {
            available: 1250000,
            held: 0,
            total: 1250000,
            locked: false,
        }
    );
}

#[test]
fn float_precision() {
    let csv = "\
        type,client,tx,amount
        deposit,1,1,1.1234
        deposit,1,2,2.0001
        withdrawal,1,3,0.1235
    ";

    let accounts = run_scenario(csv);

    assert_eq!(
        accounts[&1],
        ExpectedAccount {
            available: 30000,
            held: 0,
            total: 30000,
            locked: false,
        }
    );
}

#[test]
fn negative_available_via_dispute() {
    let csv = "\
        type,client,tx,amount
        deposit,1,1,100.00
        withdrawal,1,2,100.00
        dispute,1,1,
    ";

    let accounts = run_scenario(csv);

    assert_eq!(
        accounts[&1],
        ExpectedAccount {
            available: -1000000,
            held: 1000000,
            total: 0,
            locked: false,
        }
    );
}

#[test]
fn negative_available_via_dispute_and_withdrawal() {
    let csv = "\
        type,client,tx,amount
        deposit,1,1,100.00
        deposit,1,2,50.00
        withdrawal,1,2,100.00
        dispute,1,1,
    ";

    let accounts = run_scenario(csv);

    assert_eq!(
        accounts[&1],
        ExpectedAccount {
            available: -500000,
            held: 1000000,
            total: 500000,
            locked: false,
        }
    );
}

#[test]
fn non_sequential_tx_ids() {
    let csv = "\
        type,client,tx,amount
        deposit,1,500,100.00
        deposit,1,10,50.00
        withdrawal,1,99,20.00
        dispute,1,500,
    ";

    let accounts = run_scenario(csv);

    assert_eq!(
        accounts[&1],
        ExpectedAccount {
            available: 300000,
            held: 1000000,
            total: 1300000,
            locked: false,
        }
    );
}

#[test]
fn duplicate_tx_ids() {
    let csv = "\
        type,client,tx,amount
        deposit,1,1,100.00
        deposit,2,1,500.00
        deposit,2,2,200.00
    ";

    let accounts = run_scenario(csv);

    assert_eq!(
        accounts[&1],
        ExpectedAccount {
            available: 1000000,
            held: 0,
            total: 1000000,
            locked: false,
        }
    );
    assert_eq!(
        accounts[&2],
        ExpectedAccount {
            available: 2000000,
            held: 0,
            total: 2000000,
            locked: false,
        }
    );
}

#[test]
fn client_mismatch() {
    let csv = "\
type,client,tx,amount
        deposit,1,1,100.00
        deposit,2,2,50.00
        dispute,2,1,
        dispute,1,1,
        resolve,2,1,
    ";

    let accounts = run_scenario(csv);

    assert_eq!(
        accounts[&1],
        ExpectedAccount {
            available: 0,
            held: 1000000,
            total: 1000000,
            locked: false,
        }
    );
    assert_eq!(
        accounts[&2],
        ExpectedAccount {
            available: 500000,
            held: 0,
            total: 500000,
            locked: false,
        }
    );
}

#[test]
fn locked_account_can_dispute_other_transactions() {
    let csv = "\
        type,client,tx,amount
        deposit,1,1,100.00
        deposit,1,2,50.00
        dispute,1,1,
        chargeback,1,1,
        dispute,1,2,
        chargeback,1,2,
    ";

    let accounts = run_scenario(csv);

    assert_eq!(
        accounts[&1],
        ExpectedAccount {
            available: 0,
            held: 0,
            total: 0,
            locked: true,
        }
    );
}

#[test]
fn locked_account_can_resolve_other_transactions() {
    let csv = "\
        type,client,tx,amount
        deposit,1,1,100.00
        deposit,1,2,50.00
        dispute,1,1,
        chargeback,1,1,
        dispute,1,2,
        resolve,1,2,
    ";

    let accounts = run_scenario(csv);

    assert_eq!(
        accounts[&1],
        ExpectedAccount {
            available: 500000,
            held: 0,
            total: 500000,
            locked: true,
        }
    );
}

#[test]
fn resolve_transaction_cannot_be_re_disputed() {
    let csv = "\
        type,client,tx,amount
        deposit,1,1,100.0000
        dispute,1,1,
        resolve,1,1,
        dispute,1,1,
        chargeback,1,1,
    ";

    let accounts = run_scenario(csv);

    assert_eq!(
        accounts[&1],
        ExpectedAccount {
            available: 1000000,
            held: 0,
            total: 1000000,
            locked: false,
        }
    );
}

#[test]
fn output_format_negative_available() {
    let csv = "\
        type,client,tx,amount
        deposit,1,1,100.00
        withdrawal,1,2,100.00
        dispute,1,1,
    ";

    let output = run_scenario_output(csv);
    let lines: Vec<&str> = output.lines().collect();

    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("client"));

    let data_line = lines[1];
    assert!(
        data_line.contains("-100"),
        "Expected negative available in output, got: {}",
        data_line
    );
    assert!(
        data_line.contains("100"),
        "Expected held=100 in output, got: {}",
        data_line
    );
}

#[test]
fn test_with_files_directory_transactions_1() {
    let input_csv = std::fs::read_to_string("test_files/transaction_test.csv")
        .expect("Failed to read input CSV");

    let actual_accounts = run_scenario(&input_csv);

    let expected_csv = std::fs::read_to_string("test_files/transaction_test_Result.csv")
        .expect("Failed to read expected CSV");
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(expected_csv.as_bytes());

    let mut expected_accounts = std::collections::BTreeMap::new();
    for result in reader.records() {
        let record = result.unwrap();
        let client: u16 = record[0].parse().unwrap();
        let available: f64 = record[1].parse().unwrap();
        let held: f64 = record[2].parse().unwrap();
        let total: f64 = record[3].parse().unwrap();
        let locked: bool = record[4].parse().unwrap();

        expected_accounts.insert(
            client,
            ExpectedAccount {
                available: (available * 10000.0).round() as i64,
                held: (held * 10000.0).round() as i64,
                total: (total * 10000.0).round() as i64,
                locked,
            },
        );
    }

    assert_eq!(actual_accounts, expected_accounts);
}

#[test]
fn test_with_files_directory_transactions_2() {
    let input_csv = std::fs::read_to_string("test_files/transaction_test_2.csv")
        .expect("Failed to read input CSV");

    let actual_accounts = run_scenario(&input_csv);

    let expected_csv = std::fs::read_to_string("test_files/transaction_test_2_Result.csv")
        .expect("Failed to read expected CSV");
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(expected_csv.as_bytes());

    let mut expected_accounts = std::collections::BTreeMap::new();
    for result in reader.records() {
        let record = result.unwrap();
        let client: u16 = record[0].parse().unwrap();
        let available: f64 = record[1].parse().unwrap();
        let held: f64 = record[2].parse().unwrap();
        let total: f64 = record[3].parse().unwrap();
        let locked: bool = record[4].parse().unwrap();

        expected_accounts.insert(
            client,
            ExpectedAccount {
                available: (available * 10000.0).round() as i64,
                held: (held * 10000.0).round() as i64,
                total: (total * 10000.0).round() as i64,
                locked,
            },
        );
    }

    assert_eq!(actual_accounts, expected_accounts);
}
