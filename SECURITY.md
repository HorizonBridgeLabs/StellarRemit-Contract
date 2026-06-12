# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| 1.3.x   | ✅ Active support  |
| 1.2.x   | ✅ Active support  |
| 1.1.x   | ❌ End of life     |
| 1.0.x   | ❌ End of life     |

## Reporting a Vulnerability

**DO NOT open a public issue.** Email security concerns to the maintainers privately.

We aim to acknowledge reports within 48 hours and provide a fix within 7 days.

## Security Model

### Authorization
- **Admin-only functions:** `init`, `transfer_admin`, `pause`, `unpause`, `withdraw`, `set_fee`, `set_rate_limit`, `extend_ttl`, `batch_deposit`, `collect_fees`, `admin_release_escrow`, `admin_cancel_escrow`, `record_upgrade`, `set_daily_limit`, `add_admin`, `remove_admin`, `set_approval_threshold`
- **User-authenticated:** `deposit`, `send`, `escrow_funds`, `release_escrow`, `cancel_escrow`, `set_user_metadata`, `confirm_escrow`
- **Public reads:** `balance`, `get_transaction`, `transaction_exists`, `get_admin`, `is_paused`, `total_supply`, `tx_count`, `get_fee`, `get_rate_limit`, `get_user_metadata`, `version`, `stats`, `get_transactions_page`, `query_user_transactions`, `is_escrow_confirmed`, `get_upgrade_info`, `get_daily_limit`, `get_admin_set`, `get_approval_threshold`

### Overflow Protection
All balance arithmetic uses `checked_add` / `checked_sub` with explicit panic on overflow. No raw `+` or `-` operators on user balances.

### Rate Limiting
A configurable cooldown (default 300s) prevents rapid-fire transactions per address. Admin can adjust or disable via `set_rate_limit(0)`.

### Pause Mechanism
Admin can pause the contract to halt deposits, sends, and escrows while preserving reads and escrow releases.

### Fee Protection
- Max fee capped at 10,000 bps (100%)
- Fee-exceeds-amount panics prevent zero-value transfers
- Minimum 1-stroop fee floor prevents fee rounding to zero

### Re-initialization Guard
`init()` panics with "already initialized" if called more than once.

### Self-transfer Prevention
`sender != recipient` assertion prevents accidental self-transfers.

### Daily Volume Limits
A configurable per-address daily transfer cap (`set_daily_limit`) prevents abuse. Set to 0 to disable (default). Enforced on `send()` and `escrow_funds()`.

### Multi-Sig Admin
Multiple admin signers with configurable approval threshold via `add_admin`, `remove_admin`, `set_approval_threshold`. Backward compatible with single admin (default threshold = 1).

### Escrow Dispute Resolution
Admin functions `admin_release_escrow` and `admin_cancel_escrow` allow resolving disputed escrows, bypassing sender auth and expiry checks.

### Contract Address Guards
- `init(admin)` — refuses the contract's own address as admin
- `transfer_admin(new_admin)` — refuses the contract's own address as new admin
- `set_fee(fee_bps, treasury)` — refuses the contract's own address as treasury
- `withdraw(from, to, amount)` — refuses from==to and contract as destination

### Input Validation
- `set_user_metadata` — enforces key length ≤ 128 chars, value length ≤ 1024 chars, both non-empty
- `deposit` — enforces minimum deposit threshold (1,000,000 stroops)
- All amount parameters validated > 0 on state-changing operations

## Development Practices
- CI enforces `cargo fmt`, `cargo clippy -D warnings`, `cargo test`, and `cargo build --release`
- `#![no_std]` ensures no std imports
- `RUSTFLAGS: "-D warnings"` in CI treats all warnings as errors
