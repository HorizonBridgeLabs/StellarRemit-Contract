# StellarRemit-Contract

[![CI](https://github.com/HorizonBridgeLabs/StellarRemit-Contract/actions/workflows/ci.yml/badge.svg)](https://github.com/HorizonBridgeLabs/StellarRemit-Contract/actions)

Soroban smart contract for on-chain remittance — escrow, transfer, and balance management on Stellar.

---

## Structure

```
src/
  lib.rs              # contract entry point & all functions
  types.rs            # Transaction struct, TransactionStatus, DataKey
tests/
  integration_test.rs # unit + integration tests
Cargo.toml
deploy.sh             # build → optimize → deploy → init
```

---

## Prerequisites

- Rust stable + `wasm32-unknown-unknown` target
- [`soroban-cli`](https://developers.stellar.org/docs/tools/developer-tools/cli/stellar-cli)

```bash
rustup target add wasm32-unknown-unknown
cargo install --locked stellar-cli --features opt
```

---

## Build

```bash
cargo build --release --target wasm32-unknown-unknown
```

---

## Test

```bash
cargo test
```

---

## Deploy (Testnet)

```bash
export STELLAR_SECRET_KEY=S...
export STELLAR_PUBLIC_KEY=G...
bash deploy.sh
```

`deploy.sh` will:
1. Build the WASM
2. Optimize with `soroban contract optimize`
3. Deploy to Stellar testnet
4. Call `init` with your public key as admin

---

## Contract Functions

| Function | Auth | Description |
|---|---|---|
| `init(admin)` | admin | Initialize contract, set admin |
| `deposit(sender, amount)` | sender | Add funds to on-chain balance |
| `send(sender, recipient, amount)` | sender | Instant transfer, emits 2 events |
| `escrow_funds(sender, recipient, amount)` | sender | Lock funds pending release |
| `release_escrow(transaction_id)` | sender | Release escrowed funds to recipient |
| `get_transaction(id)` | — | Read transaction by ID |
| `balance(addr)` | — | Read address balance |

---

## Events

| Event | Emitted by | Data |
|---|---|---|
| `transfer_created` | `send`, `escrow_funds` | `(tx_id, amount)` |
| `transfer_completed` | `send`, `release_escrow` | `(tx_id, recipient)` |

---

## CLI Interactions

```bash
RPC="https://soroban-testnet.stellar.org"
PASS="Test SDF Network ; September 2015"
ID=<contract_id>
SRC=<your_secret_key>

# Deposit
soroban contract invoke --id $ID --source $SRC --rpc-url $RPC --network-passphrase "$PASS" \
  -- deposit --sender G... --amount 1000

# Send
soroban contract invoke --id $ID --source $SRC --rpc-url $RPC --network-passphrase "$PASS" \
  -- send --sender G... --recipient G... --amount 200

# Escrow
soroban contract invoke --id $ID --source $SRC --rpc-url $RPC --network-passphrase "$PASS" \
  -- escrow_funds --sender G... --recipient G... --amount 400

# Release escrow
soroban contract invoke --id $ID --source $SRC --rpc-url $RPC --network-passphrase "$PASS" \
  -- release_escrow --transaction_id 1

# Check balance
soroban contract invoke --id $ID --source $SRC --rpc-url $RPC --network-passphrase "$PASS" \
  -- balance --addr G...

# Get transaction
soroban contract invoke --id $ID --source $SRC --rpc-url $RPC --network-passphrase "$PASS" \
  -- get_transaction --transaction_id 1
```

---

## Storage Design

| Key | Storage type | Description |
|---|---|---|
| `DataKey::Admin` | Instance | Admin address |
| `DataKey::TxCount` | Instance | Auto-increment transaction counter |
| `DataKey::Balance(addr)` | Persistent | Per-address balance |
| `DataKey::Transaction(id)` | Persistent | Transaction record by ID |

---

## Contributing

See open [GitHub Issues](https://github.com/HorizonBridgeLabs/StellarRemit-Contract/issues) — 70 issues are available for contributors, labelled `good first issue`, `enhancement`, `testing`, `deployment`, `security`, and `documentation`.
