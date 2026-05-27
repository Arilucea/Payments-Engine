//! Main application entry point for the payment engine.
//!
//! This module reads a CSV file containing transactions, processes them using the `PaymentsEngine`,
//! and writes the resulting account states to standard output in CSV format.
use std::error::Error;
use std::fs::File;
use std::io::BufWriter;
use std::{env, io};

use payment_engine::engine::PaymentsEngine;
use payment_engine::{print_accounts, run_engine};

/// The main entry point.
///
/// It reads the CSV file path from the command line arguments, processes each transaction,
/// and outputs the final account balances to standard output.
fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args()
        .nth(1)
        .ok_or("Usage: cargo run -- <transactions.csv>")?;

    let file = File::open(&path)?;

    let mut engine = PaymentsEngine::new();
    run_engine(file, &mut engine)?;

    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    print_accounts(&mut out, &engine.records.accounts.storage)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    #[test]
    fn test_main_runs_successfully() {
        let test_file = "test_main_input.csv";
        let csv_data = "\
            type,client,tx,amount
            deposit,1,1,100.50
            ";
        fs::write(test_file, csv_data).unwrap();

        let output = Command::new("cargo")
            .arg("run")
            .arg("--")
            .arg(test_file)
            .output()
            .expect("Failed to execute cargo run");

        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("client, available, held, total, locked"));
        assert!(stdout.contains("1, 100.5000, 0, 100.5000, false"));

        fs::remove_file(test_file).unwrap();
    }

    #[test]
    fn test_main_missing_args() {
        let output = Command::new("cargo")
            .arg("run")
            .output()
            .expect("Failed to execute cargo run");

        assert!(!output.status.success());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("Usage: cargo run -- <transactions.csv>"));
    }
}
