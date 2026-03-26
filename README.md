# LiquiFact Contracts

Soroban smart contracts for LiquiFact invoice financing on Stellar.

## Current scope

This workspace currently ships one contract crate: `escrow`.

The escrow contract now supports **multi-escrow storage keyed by invoice id** so one deployed contract instance can manage multiple concurrent invoices.

## Workspace

```text
liquifact-contracts/
|-- Cargo.toml
|-- escrow/
|   |-- Cargo.toml
|   `-- src/
|       |-- lib.rs
|       `-- test.rs
|-- docs/
|   |-- EVENT_SCHEMA.md
|   |-- openapi.yaml
|   `-- tests/openapi.test.js
`-- .github/workflows/ci.yml
```

## Escrow storage model

`escrow/src/lib.rs` stores state with:

- `DataKey::Invoice(Symbol)` -> `InvoiceEscrow`
- `DataKey::InvoiceList` -> `Vec<Symbol>`
- `DataKey::Version` -> schema version

### Migration assumption from singleton model

A compatibility method `migrate_singleton(invoice_id)` migrates data from the old singleton key `"escrow"` into `DataKey::Invoice(invoice_id)`.

Assumptions validated by tests:

- Legacy singleton value exists under key `"escrow"`
- Legacy `invoice_id` must match the provided `invoice_id`
- Target keyed invoice must not already exist

## Contract lifecycle

Status values:

- `0` open
- `1` funded
- `2` settled

Main methods:

- `init`
- `get_escrow`
- `list_invoices`
- `fund`
- `settle`
- `update_maturity`
- `transfer_admin`
- `migrate_singleton`

## Local development

```bash
cargo fmt --all -- --check
cargo build
cargo test
```

## Notes

- Authorization is enforced per action (`admin`, `investor`, `sme_address`)
- Funding uses checked arithmetic for overflow safety
- Tests cover multi-invoice isolation, state transitions, auth capture, and singleton migration behavior
