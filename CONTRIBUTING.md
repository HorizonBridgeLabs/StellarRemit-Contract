# Contributing to StellarRemit-Contract

## Development Setup

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add wasm target
rustup target add wasm32-unknown-unknown

# Install Stellar CLI
cargo install --locked stellar-cli --features opt

# Clone and build
git clone https://github.com/HorizonBridgeLabs/StellarRemit-Contract.git
cd StellarRemit-Contract
cargo build --release --target wasm32-unknown-unknown
```

## Running Tests

```bash
cargo test
```

All tests use the soroban-sdk `testutils` for local simulation — no network required.

## Code Quality

CI runs on every PR. Before pushing, run locally:

```bash
cargo fmt --all -- --check
cargo clippy --target wasm32-unknown-unknown -- -D warnings
cargo test
cargo build --release --target wasm32-unknown-unknown
```

## Project Structure

```
src/
  lib.rs              # Contract entry point & all public functions
  types.rs            # DataKey, Transaction, TransactionStatus, FeeConfig
tests/
  integration_test.rs # 55+ unit + integration tests
  test_send_event.rs  # Event emission tests
.github/workflows/
  ci.yml              # GitHub Actions CI pipeline
```

## Storage Architecture

| Type | Purpose | Keys |
|------|---------|------|
| **Instance** | Contract-scoped, shares instance TTL | Admin, TxCount, Paused, Config, FeeConfig |
| **Persistent** | Per-address/user, independent TTL | Balance, Transaction, LastTxTime, TotalSupply, UserMetadata |

See [README.md](./README.md#storage-design) for full details.

## Conventions

- All public functions document auth requirements in doc comments
- Events are emitted for all state-changing operations
- Balance arithmetic uses `checked_add`/`checked_sub` with `expect("arithmetic overflow")`
- Rate limiting and pause guards use private helper functions
- Tests use `env.events()` for event verification and `env.ledger().set()` for time manipulation

## Issues

See [open issues](https://github.com/HorizonBridgeLabs/StellarRemit-Contract/issues) — labelled `good first issue`, `enhancement`, `testing`, `security`, and `documentation`.
