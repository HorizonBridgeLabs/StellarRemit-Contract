#![no_std]

mod types;

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};
use types::{DataKey, FeeConfig, Transaction};

pub use types::TransactionStatus;

#[contract]
pub struct RemittanceContract;

#[contractimpl]
impl RemittanceContract {
    /// Initialize contract with an admin address for the remmittanceContract.
    pub fn init(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        admin.require_auth();
        const DEFAULT_COOLDOWN: u64 = 300;
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TxCount, &0u64);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage()
            .instance()
            .set(&DataKey::Config, &DEFAULT_COOLDOWN);
    }

    /// Deposit funds into sender's on-chain balance.!!!!!!
    /// Returns the new balance after deposit.
    pub fn deposit(env: Env, sender: Address, amount: i128) -> i128 {
        sender.require_auth();
        Self::require_not_paused(&env);

        // Enhanced validation
        assert!(amount > 0, "Deposit amount must be greater than zero");
        const MIN_DEPOSIT: i128 = 1_000_000; // Minimum deposit amount (0.000001 XLM equivalent)
        assert!(
            amount >= MIN_DEPOSIT,
            "amount below minimum deposit threshold"
        );

        // Balance uses persistent() storage: per-address data must survive
        // beyond the contract instance's TTL and be independently renewable.
        let key = DataKey::Balance(sender.clone());
        // Balance uses persistent storage — survives ledger expiry
        let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_balance = balance.checked_add(amount).expect("arithmetic overflow");
        env.storage().persistent().set(&key, &new_balance);

        // Emit deposit event for tracking
        env.events().publish(
            (Symbol::new(&env, "deposit"), sender.clone()),
            (amount, new_balance),
        );

        new_balance
    }

    /// Transfer funds from sender to recipient immediately.
    pub fn send(env: Env, sender: Address, recipient: Address, amount: i128) -> u64 {
        sender.require_auth();
        assert!(amount > 0, "Send amount must be greater than zero");
        assert!(sender != recipient, "cannot send to yourself");
        Self::require_not_paused(&env);
        Self::check_rate_limit(&env, &sender);

        let (net_amount, fee_amount) = Self::compute_fee(&env, amount);
        assert!(
            net_amount > 0,
            "fee exceeds transfer amount — recipient would receive zero"
        );

        // Balance keys use persistent() storage so user funds survive instance eviction.
        let sender_key = DataKey::Balance(sender.clone());
        // Balance uses persistent storage — survives ledger expiry
        let sender_bal: i128 = env.storage().persistent().get(&sender_key).unwrap_or(0);
        assert!(sender_bal >= amount, "insufficient balance");

        let recipient_key = DataKey::Balance(recipient.clone());
        let recipient_bal: i128 = env.storage().persistent().get(&recipient_key).unwrap_or(0);

        env.storage().persistent().set(
            &sender_key,
            &sender_bal.checked_sub(amount).expect("arithmetic overflow"),
        );
        env.storage().persistent().set(
            &recipient_key,
            &recipient_bal
                .checked_add(net_amount)
                .expect("arithmetic overflow"),
        );

        // Divert fee to treasury if applicable
        Self::credit_treasury(&env, fee_amount);

        let id = Self::next_id(&env);
        let timestamp = env.ledger().timestamp();
        let tx = Transaction {
            id,
            sender: sender.clone(),
            recipient: recipient.clone(),
            amount,
            status: TransactionStatus::Completed,
            timestamp,
            expires_at: 0,
        };
        // Transaction uses persistent storage — survives ledger expiry
        env.storage()
            .persistent()
            .set(&DataKey::Transaction(id), &tx);

        env.events().publish(
            (Symbol::new(&env, "transfer_created"), sender.clone()),
            (id, amount),
        );
        env.events().publish(
            (Symbol::new(&env, "transfer_completed"), sender.clone()),
            (id, recipient),
        );
        Self::update_last_tx_time(&env, &sender);
        id
    }

    /// Lock funds in escrow pending release.
    /// `expiry_ledgers`: number of ledgers until escrow expires (0 = no expiry).
    pub fn escrow_funds(
        env: Env,
        sender: Address,
        recipient: Address,
        amount: i128,
        expiry_ledgers: u32,
    ) -> u64 {
        sender.require_auth();
        assert!(amount > 0, "Escrow amount must be greater than zero");
        assert!(sender != recipient, "cannot escrow to yourself");
        Self::require_not_paused(&env);
        Self::check_rate_limit(&env, &sender);

        // Balance uses persistent storage — survives ledger expiry
        let sender_key = DataKey::Balance(sender.clone());
        let sender_bal: i128 = env.storage().persistent().get(&sender_key).unwrap_or(0);
        assert!(sender_bal >= amount, "insufficient balance");

        env.storage().persistent().set(
            &sender_key,
            &sender_bal.checked_sub(amount).expect("arithmetic overflow"),
        );

        let id = Self::next_id(&env);
        let timestamp = env.ledger().timestamp();
        let expires_at = if expiry_ledgers > 0 {
            u64::from(env.ledger().sequence()) + u64::from(expiry_ledgers)
        } else {
            0
        };
        let tx = Transaction {
            id,
            sender: sender.clone(),
            recipient,
            amount,
            status: TransactionStatus::Escrowed,
            timestamp,
            expires_at,
        };
        // Transaction uses persistent storage — survives ledger expiry
        env.storage()
            .persistent()
            .set(&DataKey::Transaction(id), &tx);

        env.events().publish(
            (Symbol::new(&env, "transfer_created"), sender.clone()),
            (id, amount),
        );
        Self::update_last_tx_time(&env, &sender);
        id
    }

    /// Release escrowed funds to recipient.
    pub fn release_escrow(env: Env, transaction_id: u64) {
        // Transaction uses persistent storage — survives ledger expiry
        let key = DataKey::Transaction(transaction_id);
        let mut tx: Transaction = env
            .storage()
            .persistent()
            .get(&key)
            .expect("transaction not found");

        assert!(tx.status == TransactionStatus::Escrowed, "not in escrow");

        // Expiry check: if expires_at is set, current ledger must not exceed it
        if tx.expires_at > 0 {
            assert!(
                u64::from(env.ledger().sequence()) <= tx.expires_at,
                "escrow expired"
            );
        }

        tx.sender.require_auth();

        let (net_amount, fee_amount) = Self::compute_fee(&env, tx.amount);
        assert!(
            net_amount > 0,
            "fee exceeds escrow amount — recipient would receive zero"
        );

        // Balance uses persistent storage — survives ledger expiry
        let recipient_key = DataKey::Balance(tx.recipient.clone());
        let recipient_bal: i128 = env.storage().persistent().get(&recipient_key).unwrap_or(0);
        env.storage().persistent().set(
            &recipient_key,
            &recipient_bal
                .checked_add(net_amount)
                .expect("arithmetic overflow"),
        );

        Self::credit_treasury(&env, fee_amount);

        tx.status = TransactionStatus::Released;
        env.storage().persistent().set(&key, &tx);

        env.events().publish(
            (Symbol::new(&env, "transfer_completed"), tx.sender),
            (transaction_id, tx.recipient),
        );
    }

    /// Cancel an escrowed transaction and refund the sender.
    /// Only the original sender (or admin after expiry) can cancel.
    pub fn cancel_escrow(env: Env, transaction_id: u64) {
        let key = DataKey::Transaction(transaction_id);
        let mut tx: Transaction = env
            .storage()
            .persistent()
            .get(&key)
            .expect("transaction not found");

        assert!(tx.status == TransactionStatus::Escrowed, "not in escrow");

        // Allow cancellation if escrow has expired, or sender wants to cancel
        if tx.expires_at > 0 {
            assert!(
                u64::from(env.ledger().sequence()) > tx.expires_at,
                "escrow not yet expired"
            );
        }

        tx.sender.require_auth();

        // Refund the sender
        let sender_key = DataKey::Balance(tx.sender.clone());
        let sender_bal: i128 = env.storage().persistent().get(&sender_key).unwrap_or(0);
        env.storage().persistent().set(
            &sender_key,
            &sender_bal
                .checked_add(tx.amount)
                .expect("arithmetic overflow"),
        );

        tx.status = TransactionStatus::Cancelled;
        env.storage().persistent().set(&key, &tx);

        env.events().publish(
            (Symbol::new(&env, "escrow_cancelled"), tx.sender),
            (transaction_id, tx.amount),
        );
    }

    /// Read the configured admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("admin not initialized")
    }

    /// Transfer admin rights to a new address.
    /// Requires authentication from the current admin.
    /// Emits an admin_transferred event.
    pub fn transfer_admin(env: Env, new_admin: Address) {
        let current_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("admin not initialized");
        current_admin.require_auth();

        assert!(
            new_admin != current_admin,
            "new admin must differ from current admin"
        );

        env.storage().instance().set(&DataKey::Admin, &new_admin);

        env.events().publish(
            (Symbol::new(&env, "admin_transferred"), current_admin),
            new_admin,
        );
    }

    /// Pause the contract — prevents deposits, sends, and escrows.
    /// Only callable by admin. Reads and releases still work.
    pub fn pause(env: Env) {
        Self::require_admin(&env);
        assert!(
            !env.storage()
                .instance()
                .get(&DataKey::Paused)
                .unwrap_or(false),
            "contract already paused"
        );
        env.storage().instance().set(&DataKey::Paused, &true);

        env.events()
            .publish((Symbol::new(&env, "contract_paused"),), ());
    }

    /// Unpause the contract — re-enables deposits, sends, and escrows.
    /// Only callable by admin.
    pub fn unpause(env: Env) {
        Self::require_admin(&env);
        assert!(
            env.storage()
                .instance()
                .get(&DataKey::Paused)
                .unwrap_or(false),
            "contract not paused"
        );
        env.storage().instance().set(&DataKey::Paused, &false);

        env.events()
            .publish((Symbol::new(&env, "contract_unpaused"),), ());
    }

    /// Check whether the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    /// Admin-only: set the rate limit cooldown in seconds. Use 0 to disable.
    pub fn set_rate_limit(env: Env, cooldown_seconds: u64) {
        Self::require_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::Config, &cooldown_seconds);

        env.events()
            .publish((Symbol::new(&env, "rate_limit_updated"),), cooldown_seconds);
    }

    /// Read the current rate limit cooldown in seconds.
    pub fn get_rate_limit(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .unwrap_or(300)
    }

    /// Admin-only: withdraw funds from the contract (e.g., collected fees).
    /// `from` is the address whose balance to draw from; `to` receives the funds.
    pub fn withdraw(env: Env, from: Address, to: Address, amount: i128) {
        Self::require_admin(&env);
        assert!(amount > 0, "withdraw amount must be greater than zero");

        let from_key = DataKey::Balance(from.clone());
        let from_bal: i128 = env.storage().persistent().get(&from_key).unwrap_or(0);
        assert!(from_bal >= amount, "insufficient balance");

        let to_key = DataKey::Balance(to.clone());
        let to_bal: i128 = env.storage().persistent().get(&to_key).unwrap_or(0);

        env.storage().persistent().set(
            &from_key,
            &from_bal.checked_sub(amount).expect("arithmetic overflow"),
        );
        env.storage().persistent().set(
            &to_key,
            &to_bal.checked_add(amount).expect("arithmetic overflow"),
        );

        env.events()
            .publish((Symbol::new(&env, "withdraw"), from), (to, amount));
    }

    /// Read a transaction by ID...
    pub fn get_transaction(env: Env, transaction_id: u64) -> Transaction {
        env.storage()
            .persistent()
            .get(&DataKey::Transaction(transaction_id))
            .expect("transaction not found")
    }

    /// Read balance for an address.
    pub fn balance(env: Env, addr: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Balance(addr))
            .unwrap_or(0)
    }

    /// Read the current transaction count.
    pub fn tx_count(env: Env) -> u64 {
        env.storage().instance().get(&DataKey::TxCount).unwrap_or(0)
    }

    /// Admin-only: configure the fee (basis points) and treasury address.
    /// Set fee_bps to 0 to disable fees. Max: 10_000 bps (100%).
    pub fn set_fee(env: Env, fee_bps: u32, treasury: Address) {
        Self::require_admin(&env);
        assert!(fee_bps <= 10_000, "fee basis points must not exceed 10_000");

        let config = FeeConfig {
            fee_bps,
            treasury: treasury.clone(),
        };
        env.storage().instance().set(&DataKey::FeeConfig, &config);

        env.events()
            .publish((Symbol::new(&env, "fee_updated"),), (fee_bps, treasury));
    }

    /// Read the current fee configuration.
    pub fn get_fee(env: Env) -> FeeConfig {
        if env.storage().instance().has(&DataKey::FeeConfig) {
            env.storage().instance().get(&DataKey::FeeConfig).unwrap()
        } else {
            let admin: Address = env
                .storage()
                .instance()
                .get(&DataKey::Admin)
                .expect("admin not initialized");
            FeeConfig {
                fee_bps: 0,
                treasury: admin,
            }
        }
    }

    // ── helpers ───────────────────────────────────────────

    fn next_id(env: &Env) -> u64 {
        let count: u64 = env.storage().instance().get(&DataKey::TxCount).unwrap_or(0);
        let next = count + 1;
        env.storage().instance().set(&DataKey::TxCount, &next);
        next
    }

    fn require_admin(env: &Env) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("admin not initialized");
        admin.require_auth();
    }

    fn require_not_paused(env: &Env) {
        assert!(
            !env.storage()
                .instance()
                .get(&DataKey::Paused)
                .unwrap_or(false),
            "contract is paused"
        );
    }

    /// Enforce a cooldown between consecutive send/escrow operations per address.
    /// Default cooldown: 300 seconds (5 minutes).
    fn check_rate_limit(env: &Env, addr: &Address) {
        let cooldown: u64 = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .unwrap_or(300);
        // 0 means rate limiting is disabled
        if cooldown == 0 {
            return;
        }
        let key = DataKey::LastTxTime(addr.clone());
        let now = env.ledger().timestamp();
        if let Some(last_time) = env.storage().persistent().get::<DataKey, u64>(&key) {
            assert!(
                now >= last_time && now - last_time >= cooldown,
                "rate limit exceeded — wait before next operation"
            );
        }
    }

    /// Record the last operation timestamp after a successful send or escrow.
    fn update_last_tx_time(env: &Env, sender: &Address) {
        let key = DataKey::LastTxTime(sender.clone());
        env.storage()
            .persistent()
            .set(&key, &env.ledger().timestamp());
    }

    /// Compute the net amount after fee and the fee amount itself.
    /// Returns (net_to_recipient, fee_to_treasury).
    fn compute_fee(env: &Env, amount: i128) -> (i128, i128) {
        if !env.storage().instance().has(&DataKey::FeeConfig) {
            return (amount, 0);
        }
        let config: FeeConfig = env.storage().instance().get(&DataKey::FeeConfig).unwrap();
        if config.fee_bps == 0 {
            return (amount, 0);
        }
        let fee_amount = amount
            .checked_mul(config.fee_bps as i128)
            .expect("arithmetic overflow")
            / 10_000i128;
        // Ensure at least 1 stroop fee if fee_bps > 0 but fee rounds to 0
        let fee_amount = if fee_amount == 0 { 1 } else { fee_amount };
        let net_amount = amount.checked_sub(fee_amount).expect("arithmetic overflow");
        (net_amount, fee_amount)
    }

    /// Credit the treasury with a fee amount if non-zero.
    fn credit_treasury(env: &Env, fee_amount: i128) {
        if fee_amount == 0 || !env.storage().instance().has(&DataKey::FeeConfig) {
            return;
        }
        let config: FeeConfig = env.storage().instance().get(&DataKey::FeeConfig).unwrap();
        let treasury_key = DataKey::Balance(config.treasury);
        let treasury_bal: i128 = env.storage().persistent().get(&treasury_key).unwrap_or(0);
        env.storage().persistent().set(
            &treasury_key,
            &treasury_bal
                .checked_add(fee_amount)
                .expect("arithmetic overflow"),
        );
    }
}
