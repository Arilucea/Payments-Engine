# Payments Engine

A streaming payments engine that processes transactions from a CSV file, manages client account balances, and outputs the final account state as a CSV.

## 1. Project Overview

This project implements a robust, stateful transaction processing engine in Rust. It reads a chronological stream of transactions (deposits, withdrawals, disputes, resolves, and chargebacks) from a CSV, maintains the state of client accounts, and outputs the final balances. 

The system is designed with a focus on correctness, bounded resource usage, and precise financial arithmetic, treating the input CSV order as the absolute source of truth.

## 2. Transaction Semantics

- **Chronological Truth:** The order of rows in the CSV is the strict chronological order of events. Transaction IDs are globally unique but not necessarily monotonically increasing. Client IDs are unordered.
- **Deposits:** Increase available and total funds. Saved for potential future disputes.
- **Withdrawals:** Decrease available and total funds if sufficient available funds exist.
- **Disputes:** Hold funds corresponding to a previous deposit. Decrease available, increase held.
- **Resolves:** Release held funds from a dispute. Increase available, decrease held.
- **Chargebacks:** Reverse a disputed deposit. Decrease held, decrease total, and permanently **lock** the account.
- **Locked Accounts:** Once an account is locked via a chargeback, no further deposits or withdrawals for that account are processed.

## 3. Assumptions and Interpretation of the Specification

- A dispute can only refer to a deposit.
- A resolve or chargeback can only occur if the referenced transaction is currently under dispute.
- If a withdrawal exceeds available funds, it is ignored.
- If an account is locked, all subsequent transactions (deposit/withdrawals) for that account are ignored. Disputes/resolves/chargebacks still can refer to a previous transaction.
- A transaction can only be disputed once. If a transaction is disputed more than once, it will be ignored.
- Negative balances are mathematically possible during a chargeback (if the user withdrew funds before the chargeback occurred).

## 4. Tradeoffs and Design Decisions

- **Streaming IO:** The application processes the input file sequentially using the `csv` crate.
- **Logic Isolated:** The core logic is isolated in the `PaymentEngine`, making it easy to reuse in other contexts. eg a tcp server instead of cli.
- **Storing Only Deposits:** Withdrawals are not stored in the `TxId` map since they cannot be disputed.
- **Synchronous Processing:** The engine is intentionally synchronous. Because transaction processing is stateful and strict ordering is required, asynchronous processing (e.g., via Tokio) within the core state machine introduces unnecessary overhead and complexity.
- **Precision Handling:** Financial arithmetic requires exact precision to prevent floating-point anomalies. The engine uses the `rust_decimal` crate for csv processing and `i64` for internal calculations to improve performance and correctness. Output serialization rounds to 4 decimal places as required by the specification.
- **Max Value Handling:** The `i64` type in Rust has a maximum value of `2^63 - 1`. If a deposit exceeds this value, it will return an error and the transaction will be ignored.

## 5. Error Handling

The engine adopts a "graceful degradation" philosophy for invalid data.
- **Protocol Violations:** Invalid disputes, resolves, or chargebacks (e.g., referencing non-existent transactions, or transactions not in the correct state) are intentionally ignored per the specification.
- **Malformed Data:** Rows that cannot be parsed are skipped with an error logged to `stderr`. Processing continues for the remainder of the file.

## 6. Example CLI Usage

The application takes a single positional argument: the path to the input CSV file. The resulting account states are printed directly to `stdout`.
Given an `input.csv` containing transactions:

```bash
cargo run -- transactions.csv > accounts.csv
```

Errors and warnings (e.g., malformed lines or invalid operations) are written to `stderr` and will not pollute the `stdout` CSV output.

## 7. Data Model

The core state is maintained in memory using efficient hash maps:
- `HashMap<ClientId, Account>`: Tracks the state of each client (available, held, total balances, and locked status).
- `HashMap<TxId, DepositRecord>`: Stores historical deposits.

**Invariant:** `total = available + held` is strictly maintained for every account across all operations.

## 8. Testing Strategy

The engine is covered by a test suite focusing on:
- **Unit Tests:** Validating the strict state transitions of individual transactions.
- **Integration Tests:** Passing full CSV mock data through the engine and validating the final `stdout` CSV against expected results.

To execute all the tests:
```bash
cargo test
```

