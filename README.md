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
| `send(sender, recipient, amount)` | sender | Instant transfer (rate-limited, paused-guarded) |
| `escrow_funds(sender, recipient, amount, expiry_ledgers)` | sender | Lock funds pending release (rate-limited) |
| `release_escrow(tx_id)` | sender | Release escrowed funds to recipient |
| `cancel_escrow(tx_id)` | sender | Cancel escrow and refund sender (only after expiry) |
| `transfer_admin(new_admin)` | admin | Transfer admin rights |
| `pause()` | admin | Pause deposits, sends, and escrows |
| `unpause()` | admin | Resume operations |
| `withdraw(from, to, amount)` | admin | Admin withdraw from any balance |
| `get_admin()` | — | Read current admin address |
| `get_transaction(tx_id)` | — | Read transaction by ID |
| `transaction_exists(tx_id)` | — | Check if a transaction exists (no panic) |
| `balance(addr)` | — | Read address balance |
| `tx_count()` | — | Read total transaction count |
| `is_paused()` | — | Check if contract is paused |
| `total_supply()` | — | Read total deposited funds |
| `set_rate_limit(seconds)` | admin | Change rate limit cooldown (0 = disable) |
| `get_rate_limit()` | — | Read current rate limit cooldown |
| `set_fee(bps, treasury)` | admin | Configure fee percentage and treasury |
| `get_fee()` | — | Read current fee configuration |
| `extend_ttl(ledgers)` | admin | Extend persistent storage TTL |
| `set_user_metadata(user, key, value)` | user | Store per-user metadata (validated lengths) |
| `get_user_metadata(user)` | — | Read user metadata |
| `version()` | — | Return contract version string |
| `stats()` | — | Aggregate (tx_count, supply, paused, cooldown) |

---

## Events

| Event | Emitted by | Data |
|---|---|---|
| `deposit` | `deposit` | `(amount, new_balance)` |
| `transfer_created` | `send`, `escrow_funds` | `(tx_id, amount)` |
| `transfer_completed` | `send`, `release_escrow` | `(tx_id, recipient)` |
| `escrow_cancelled` | `cancel_escrow` | `(tx_id, amount)` |
| `admin_transferred` | `transfer_admin` | `new_admin` |
| `contract_paused` | `pause` | `()` |
| `contract_unpaused` | `unpause` | `()` |
| `withdraw` | `withdraw` | `(to, amount)` |
| `fee_updated` | `set_fee` | `(fee_bps, treasury)` |
| `rate_limit_updated` | `set_rate_limit` | `cooldown_seconds` |
| `ttl_extended` | `extend_ttl` | `ledgers` |
| `metadata_updated` | `set_user_metadata` | `(user, key)` |

---

## Generating TypeScript Bindings

For frontend integration, generate TypeScript bindings from the deployed contract:

```bash
# Install the soroban CLI bindings plugin
cargo install --locked soroban-cli

# Generate bindings for your deployed contract
soroban contract bindings typescript \
  --contract-id <CONTRACT_ID> \
  --output-dir ./bindings \
  --rpc-url https://soroban-testnet.stellar.org \
  --network-passphrase "Test SDF Network ; September 2015"
```

The generated bindings include typed methods for all contract functions, making it easy to integrate with React, Next.js, or any TypeScript frontend.

```typescript
// Example: using generated bindings in a TypeScript project
import { Client as RemittanceClient } from './bindings';

const client = new RemittanceClient({
  contractId: '<CONTRACT_ID>',
  rpcUrl: 'https://soroban-testnet.stellar.org',
  networkPassphrase: 'Test SDF Network ; September 2015',
});

// Now call contract functions with full type safety
const balance = await client.balance({ addr: userPublicKey });
```

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

# Cancel escrow
soroban contract invoke --id $ID --source $SRC --rpc-url $RPC --network-passphrase "$PASS" \
  -- cancel_escrow --transaction_id 1

# Pause / unpause
soroban contract invoke --id $ID --source $SRC --rpc-url $RPC --network-passphrase "$PASS" \
  -- pause

# Set fee (2.5% to treasury)
soroban contract invoke --id $ID --source $SRC --rpc-url $RPC --network-passphrase "$PASS" \
  -- set_fee --fee_bps 250 --treasury G...

# Set rate limit (60s cooldown)
soroban contract invoke --id $ID --source $SRC --rpc-url $RPC --network-passphrase "$PASS" \
  -- set_rate_limit --cooldown_seconds 60

# Extend storage TTL
soroban contract invoke --id $ID --source $SRC --rpc-url $RPC --network-passphrase "$PASS" \
  -- extend_ttl --ledgers 535680

# Get version
soroban contract invoke --id $ID --rpc-url $RPC --network-passphrase "$PASS" \
  -- version

# Get aggregate stats
soroban contract invoke --id $ID --rpc-url $RPC --network-passphrase "$PASS" \
  -- stats

# Get total supply
soroban contract invoke --id $ID --rpc-url $RPC --network-passphrase "$PASS" \
  -- total_supply

# Set user metadata
soroban contract invoke --id $ID --source $SRC --rpc-url $RPC --network-passphrase "$PASS" \
  -- set_user_metadata --user G... --key kyc_status --value verified

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
| `DataKey::Paused` | Instance | Contract pause state (bool) |
| `DataKey::Balance(addr)` | Persistent | Per-address balance |
| `DataKey::Transaction(id)` | Persistent | Transaction record by ID |
| `DataKey::LastTxTime(addr)` | Persistent | Last operation timestamp for rate limiting |
| `DataKey::TotalSupply` | Persistent | Total deposited funds tracker |
| `DataKey::FeeConfig` | Instance | Fee rate and treasury address |
| `DataKey::UserMetadata(addr)` | Persistent | Per-user key-value metadata |

---

## Contributing

See open [GitHub Issues](https://github.com/HorizonBridgeLabs/StellarRemit-Contract/issues) — 70 issues are available for contributors, labelled `good first issue`, `enhancement`, `testing`, `deployment`, `security`, and `documentation`.
