# LiquiFact Contracts

Soroban smart contracts for LiquiFact, the invoice liquidity network on Stellar. This repository currently contains the `escrow` contract that holds investor funds for tokenized invoices until settlement.

## Prerequisites

- Rust 1.70+ (stable)
- Soroban CLI (optional for deployment)

For local development and CI, Rust is enough.

## Setup

```bash
cargo build
cargo test
```

## Development

| Command | Description |
|---|---|
| `cargo build` | Build the workspace |
| `cargo test` | Run unit tests |
| `cargo fmt` | Format code |
| `cargo fmt -- --check` | Check formatting |

## Project structure

```text
liquifact-contracts/
|-- Cargo.toml
|-- README.md
`-- escrow/
    |-- Cargo.toml
    `-- src/
        |-- lib.rs
        `-- test.rs
```

## Escrow contract

- `init`: Create an invoice escrow.
- `get_escrow`: Read the current escrow state.
- `fund`: Record funding, track each investor's principal contribution, and mark the escrow funded once the target is reached.
- `settle`: Mark a funded escrow as settled.
- `get_investor_count`: Return the number of distinct investors recorded for the escrow.
- `get_investor_contribution`: Return the principal amount recorded for one investor.
- `max_investors`: Return the supported investor cap for one escrow.

## Storage guardrails

The escrow stores a per-investor contribution map inside the contract instance. That map is intentionally bounded.

- Supported investor cardinality: `128` distinct investors per escrow
- Product assumption: invoices that need more than `128` backers should be split across multiple escrows or a higher-level allocation flow
- Security goal: prevent denial-of-storage attacks that keep inserting new investor keys until a single contract-data entry becomes too large or too expensive to update

The regression tests in `escrow/src/test.rs` enforce these assumptions:

- The `129th` distinct investor is rejected.
- Re-funding an existing investor at the cap is still allowed.
- At `128` investors, the serialized investor map and escrow entry must stay below documented byte thresholds.
- The final insertion at the cap must stay within a bounded write footprint.

These limits are designed to keep the contract well below Soroban's contract-data entry limits and to catch future schema changes that would bloat per-investor storage.

## Security notes

- Funding amounts must be positive.
- Distinct investor growth is capped per escrow.
- Funding totals and investor balances use checked addition to avoid overflow.
- Storage-growth tests act as regression guards against accidental state bloat.

## CI

Run these before opening a PR:

```bash
cargo fmt --all -- --check
cargo build
cargo test
```

## License

MIT
