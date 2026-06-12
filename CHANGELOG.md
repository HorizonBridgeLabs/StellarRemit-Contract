# Changelog

All notable changes to StellarRemit-Contract.

---

## [1.3.0] — 2026-06-12

### Added
- **`memo` field** — Optional payment reference (max 64 bytes) on Transaction struct, supported in `send()` and `escrow_funds()` (#109)
- **`get_transactions_page(offset, limit)`** — Paginated transaction queries with max 50 per page (#113)
- **`query_user_transactions(user, limit, offset)`** — Query all transactions for a user address (#110)
- **`batch_deposit(recipients, amounts)`** — Admin multi-recipient deposit in a single call (#112)
- **`collect_fees(to)`** — Admin collect accumulated treasury fees to any address (#118)
- **`admin_release_escrow(tx_id)`** / **`admin_cancel_escrow(tx_id)`** — Admin dispute resolution overrides (#115)
- **`confirm_escrow(tx_id)`** / **`is_escrow_confirmed(tx_id)`** — Recipient confirmation flow for memo'd escrows (#117)
- **`record_upgrade(version)`** / **`get_upgrade_info()`** — On-chain upgrade history tracking (#114)
- **`set_daily_limit(limit)`** / **`get_daily_limit()`** — Per-address daily transfer volume enforcement (#111)
- **`add_admin(addr)`** / **`remove_admin(addr)`** / **`set_approval_threshold(n)`** — Multi-sig admin support (#116)
- **`get_admin_set()`** / **`get_approval_threshold()`** — Multi-sig admin queries

### Changed
- `send()` and `escrow_funds()` now accept optional `memo` parameter
- `release_escrow()` enforces recipient confirmation when memo is present
- Both `send()` and `escrow_funds()` enforce daily volume limits when configured

### Events
- `batch_deposit`, `fees_collected`, `admin_escrow_released`, `admin_escrow_cancelled`
- `escrow_confirmed`, `contract_upgraded`, `daily_limit_updated`
- `admin_added`, `admin_removed`, `threshold_updated`

### Storage
- 5 new DataKey variants: DailyVolume, DailyLimit, UpgradeHistory, AdminSet, ApprovalThreshold

---

## [1.2.0] — 2026-06-12

### Added
- **`transaction_exists(tx_id)`** — Check if a transaction exists without panicking
- **`TransactionStatus::Pending`** — Now used in escrow_funds flow before transitioning to Escrowed

### Changed
- **`extend_ttl(ledgers)`** — Now also extends Balance and UserMetadata entries for all addresses found in transactions
- **`set_user_metadata(user, key, value)`** — Added key/value length validation (max 128/1024 chars)
- **`init(admin)`** — Added guard preventing admin from being the contract itself
- **`transfer_admin(new_admin)`** — Added guard preventing new admin from being the contract itself
- **`set_fee(fee_bps, treasury)`** — Added guard preventing treasury from being the contract itself
- **`withdraw(from, to, amount)`** — Added from!=to guard and contract-destination guard

### Security
- Comprehensive require_auth() audit and improvements (#70)
- Contract address guards on all admin/sensitive functions
- Input validation hardening for metadata, deposits, and transfers

---

## [1.1.0] — 2026-06-11

### Added
- **CI pipeline** — GitHub Actions with fmt, clippy, check, test, build, no_std verification
- **`transfer_admin(new_admin)`** — Admin can transfer ownership rights
- **`cancel_escrow(tx_id)`** — Cancel escrowed transaction and refund sender
- **`pause()` / `unpause()`** — Admin-controlled contract pause
- **`is_paused()`** — Read contract pause state
- **Sender ≠ recipient guard** — Prevent self-transfers in `send()` and `escrow_funds()`
- **Rate limiting** — 300s cooldown between send/escrow operations per address
- **`set_rate_limit(seconds)`** — Admin-configurable cooldown (0 to disable)
- **`get_rate_limit()`** — Read current cooldown
- **Admin `withdraw(from, to, amount)`** — Move funds between any addresses
- **Fee system** — `FeeConfig` struct: configurable basis points + treasury address
- **`set_fee(fee_bps, treasury)`** — Admin fee configuration
- **`get_fee()`** — Read current fee config
- **`total_supply()`** — Track total deposited funds
- **`extend_ttl(ledgers)`** — Admin extends persistent storage TTL
- **`set_user_metadata(user, key, value)`** — Per-user key-value storage
- **`get_user_metadata(user)`** — Read user metadata
- **`version()`** — Return contract version string

### Changed
- **Overflow protection** — Replaced raw `+`/`-` with `checked_add`/`checked_sub` on all balance operations
- **README** — Comprehensive refresh: 23 functions, 12 events, 9 storage keys, CLI examples
- **TypeScript bindings docs** — Added to README

### Fixed
- `TransactionStatus` import conflict in `lib.rs`
- Rate limiter timestamp now recorded only after successful operations
- `init()` explicitly sets `Paused=false`

---

## [1.0.0] — Initial

### Core
- `init(admin)` — Contract initialization
- `deposit(sender, amount)` — Add funds to on-chain balance
- `send(sender, recipient, amount)` — Instant transfer
- `escrow_funds(sender, recipient, amount, expiry_ledgers)` — Lock funds in escrow
- `release_escrow(tx_id)` — Release escrowed funds
- `get_admin()` — Read admin address
- `get_transaction(tx_id)` — Read transaction by ID
- `balance(addr)` — Read address balance
- `tx_count()` — Read transaction count

### Events
- `deposit`, `transfer_created`, `transfer_completed`
