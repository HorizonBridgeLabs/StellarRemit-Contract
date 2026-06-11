# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| 1.1.x   | ✅ Active support  |
| 1.0.x   | ❌ End of life     |

## Reporting a Vulnerability

**DO NOT open a public issue.** Email security concerns to the maintainers privately.

We aim to acknowledge reports within 48 hours and provide a fix within 7 days.

## Security Model

### Authorization
- **Admin-only functions:** `init`, `transfer_admin`, `pause`, `unpause`, `withdraw`, `set_fee`, `set_rate_limit`, `extend_ttl`
- **User-authenticated:** `deposit`, `send`, `escrow_funds`, `release_escrow`, `cancel_escrow`, `set_user_metadata`
- **Public reads:** `balance`, `get_transaction`, `get_admin`, `is_paused`, `total_supply`, `tx_count`, `get_fee`, `get_rate_limit`, `get_user_metadata`, `version`, `stats`

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

## Development Practices
- CI enforces `cargo fmt`, `cargo clippy -D warnings`, `cargo test`, and `cargo build --release`
- `#![no_std]` ensures no std imports
- `RUSTFLAGS: "-D warnings"` in CI treats all warnings as errors
