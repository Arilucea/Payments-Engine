use std::collections::HashMap;
use std::error::Error;
use std::io::{Read, Write};

use crate::engine::PaymentsEngine;
use crate::types::{Account, RawTransaction};
use csv::ReaderBuilder;

pub mod engine;
pub mod types;

/// Runs the payment engine over a given reader, processing all transactions.
///
/// # Arguments
///
/// * `reader` - The reader to read transactions from.
/// * `engine` - The payment engine to process transactions with.
pub fn run_engine<R: Read>(reader: R, engine: &mut PaymentsEngine) -> Result<(), Box<dyn Error>> {
    let mut reader = ReaderBuilder::new()
        .trim(csv::Trim::All)
        .flexible(true)
        .from_reader(reader);

    for result in reader.deserialize::<RawTransaction>() {
        match result {
            Ok(raw_tx) => {
                let e = engine.process_transaction(&raw_tx);
                if let Err(e) = e {
                    eprintln!("Error processing transaction: {:?}", e);
                }
            }
            Err(e) => {
                eprintln!("Skipping malformed row: {}", e);
            }
        }
    }
    Ok(())
}

/// Prints the final state of all accounts to the given writer in CSV format.
///
/// # Arguments
///
/// * `out` - The output writer.
/// * `accounts` - The map of account IDs to `Account` records.
pub fn print_accounts<W: Write>(
    out: &mut W,
    accounts: &HashMap<u16, Account>,
) -> Result<(), Box<dyn Error>> {
    writeln!(out, "client, available, held, total, locked")?;

    for (&client, account) in accounts {
        write!(out, "{}", client)?;
        write!(out, ", ")?;

        write_scaled_i64(out, account.available, 10000)?;
        write!(out, ", ")?;

        write_scaled_i64(out, account.held, 10000)?;
        write!(out, ", ")?;

        write_scaled_i64(out, account.total(), 10000)?;
        write!(out, ", ")?;

        writeln!(out, "{}", account.locked)?;
    }

    Ok(())
}

/// Writes a scaled integer out as a formatted decimal string (e.g., to CSV).
pub fn write_scaled_i64<W: std::io::Write>(
    out: &mut W,
    value: i64,
    scale: i64,
) -> std::io::Result<()> {
    let sign = if value < 0 { "-" } else { "" };
    let abs = value.abs();

    let int_part = abs / scale;
    let frac_part = abs % scale;

    // write integer part
    write!(out, "{}{}", sign, int_part)?;

    // only write fractional part if needed
    if frac_part != 0 {
        // assumes 4 decimal places like your MULTIPLIER
        write!(out, ".{:0width$}", frac_part, width = 4)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::PaymentsEngine;
    use crate::types::Account;
    use std::collections::HashMap;
    use std::io::Cursor;

    #[test]
    fn test_write_scaled_i64() {
        let mut buf = Vec::new();
        write_scaled_i64(&mut buf, 150500, 10000).unwrap();
        assert_eq!(String::from_utf8(buf).unwrap(), "15.0500");

        let mut buf2 = Vec::new();
        write_scaled_i64(&mut buf2, 100000, 10000).unwrap();
        assert_eq!(String::from_utf8(buf2).unwrap(), "10");

        let mut buf3 = Vec::new();
        write_scaled_i64(&mut buf3, 5, 10000).unwrap();
        assert_eq!(String::from_utf8(buf3).unwrap(), "0.0005");
    }

    #[test]
    fn test_run_engine_valid_data() {
        let csv_data = "\
            type,client,tx,amount
            deposit,1,1,100.50
            withdrawal,1,2,50.25
            ";
        let mut engine = PaymentsEngine::new();
        let cursor = Cursor::new(csv_data);
        run_engine(cursor, &mut engine).unwrap();

        let acc = engine.records.accounts.get(1).unwrap();
        assert_eq!(acc.available, 502500);
    }

    #[test]
    fn test_run_engine_malformed_data() {
        let csv_data = "\
                type,client,tx,amount
                deposit,1,1,100.50
                invalid_type,1,2,50.00
                deposit,1,not_a_number,50.00
                deposit,1,3,20.00
                ";
        let mut engine = PaymentsEngine::new();
        let cursor = Cursor::new(csv_data);
        run_engine(cursor, &mut engine).unwrap();

        let acc = engine.records.accounts.get(1).unwrap();
        assert_eq!(acc.available, 1205000);
    }

    #[test]
    fn test_print_accounts() {
        let mut accounts = HashMap::new();
        accounts.insert(
            1,
            Account {
                available: 502500,
                held: 100000,
                locked: false,
            },
        );

        let mut buf = Vec::new();
        print_accounts(&mut buf, &accounts).unwrap();

        let output = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = output.lines().collect();

        assert_eq!(lines[0], "client, available, held, total, locked");
        assert_eq!(lines[1], "1, 50.2500, 10, 60.2500, false");
    }
}
