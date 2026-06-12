#![no_std]
//! # StellarRemit Contract
//!
//! Soroban smart contract for on-chain remittance on Stellar.
//! Supports deposits, instant transfers, escrow with expiry,
//! configurable fees, rate limiting, and pause mechanism.

mod types;

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};
use types::{DataKey, FeeConfig, Transaction};

pub use types::TransactionStatus;

#[contract]
pub struct RemittanceContract;

#[contractimpl]
impl RemittanceContract {
    /// Initialize contract with an admin address for the remmittanceContract.
    ///
    /// # Security
    /// - Requires admin auth for initialization
    /// - Guards against double initialization
    /// - Validates admin is not the contract itself
    pub fn init(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        admin.require_auth();
        assert!(
            admin != env.current_contract_address(),
            "admin cannot be the contract itself"
        );
        const DEFAULT_COOLDOWN: u64 = 300;
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TxCount, &0u64);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage()
            .instance()
            .set(&DataKey::Config, &DEFAULT_COOLDOWN);
        env.storage()
            .persistent()
            .set(&DataKey::TotalSupply, &0i128);
    }

    /// Deposit funds into sender's on-chain balance.
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

        // Track total supply
        Self::add_supply(&env, amount);

        new_balance
    }

    /// Transfer funds from sender to recipient immediately.
    /// `memo`: optional payment reference (max 64 bytes).
    pub fn send(env: Env, sender: Address, recipient: Address, amount: i128, memo: Option<soroban_sdk::Bytes>) -> u64 {
        sender.require_auth();
        assert!(amount > 0, "Send amount must be greater than zero");
        assert!(sender != recipient, "cannot send to yourself");
        Self::require_not_paused(&env);
        Self::check_rate_limit(&env, &sender);
        Self::check_daily_volume(&env, &sender, amount);

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
            memo,
            recipient_confirmed: false,
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
    /// `memo`: optional payment reference (max 64 bytes).
    pub fn escrow_funds(
        env: Env,
        sender: Address,
        recipient: Address,
        amount: i128,
        expiry_ledgers: u32,
        memo: Option<soroban_sdk::Bytes>,
    ) -> u64 {
        sender.require_auth();
        assert!(amount > 0, "Escrow amount must be greater than zero");
        assert!(sender != recipient, "cannot escrow to yourself");
        Self::require_not_paused(&env);
        Self::check_rate_limit(&env, &sender);
        Self::check_daily_volume(&env, &sender, amount);

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
        // Create transaction with Pending status first, then move to Escrowed
        let mut tx = Transaction {
            id,
            sender: sender.clone(),
            recipient,
            amount,
            status: TransactionStatus::Pending,
            timestamp,
            expires_at,
            memo,
            recipient_confirmed: false,
        };
        // Transition to Escrowed after creation
        tx.status = TransactionStatus::Escrowed;
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
    /// If the escrow has a memo, recipient confirmation is required before release.
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

        // Require recipient confirmation if memo is present (confirmation flow)
        if tx.memo.is_some() {
            assert!(tx.recipient_confirmed, "recipient must confirm escrow first");
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
    ///
    /// # Security
    /// Validates that the new admin is not the same as the current admin
    /// and that the new admin address is not the contract itself.
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
        assert!(
            new_admin != env.current_contract_address(),
            "admin cannot be the contract itself"
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
    ///
    /// # Security
    /// Requires admin auth. Validates that from != to (no self-transfer).
    /// The destination address must not be the contract itself.
    pub fn withdraw(env: Env, from: Address, to: Address, amount: i128) {
        Self::require_admin(&env);
        assert!(amount > 0, "withdraw amount must be greater than zero");
        assert!(from != to, "cannot withdraw to same address");
        assert!(
            to != env.current_contract_address(),
            "cannot withdraw to contract itself"
        );

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

        // Track total supply deduction
        Self::sub_supply(&env, amount);

        env.events()
            .publish((Symbol::new(&env, "withdraw"), from), (to, amount));
    }

    /// Read the total supply of funds held in the contract.
    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0)
    }

    /// Return the contract version string.
    pub fn version(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, "1.3.0")
    }

    /// Return aggregate contract statistics in a single call.
    /// Returns (tx_count, total_supply, is_paused, rate_limit_cooldown).
    pub fn stats(env: Env) -> (u64, i128, bool, u64) {
        let tx_count: u64 = env.storage().instance().get(&DataKey::TxCount).unwrap_or(0);
        let supply: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0);
        let paused: bool = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        let cooldown: u64 = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .unwrap_or(300);
        (tx_count, supply, paused, cooldown)
    }

    /// Store arbitrary user metadata (e.g., KYC status, profile info).
    /// Only the user themselves can set their own metadata.
    ///
    /// # Security
    /// Requires user auth. Validates that the key is not empty and
    /// the value length does not exceed reasonable bounds.
    pub fn set_user_metadata(
        env: Env,
        user: Address,
        key: soroban_sdk::String,
        value: soroban_sdk::String,
    ) {
        user.require_auth();
        assert!(!key.is_empty(), "metadata key must not be empty");
        assert!(!value.is_empty(), "metadata value must not be empty");
        assert!(key.len() <= 128, "metadata key exceeds maximum length");
        assert!(value.len() <= 1024, "metadata value exceeds maximum length");
        let storage_key = DataKey::UserMetadata(user.clone());
        env.storage()
            .persistent()
            .set(&storage_key, &(key.clone(), value));

        env.events()
            .publish((Symbol::new(&env, "metadata_updated"), user), key);
    }

    /// Read user metadata. Returns None if no metadata has been set.
    pub fn get_user_metadata(
        env: Env,
        user: Address,
    ) -> Option<(soroban_sdk::String, soroban_sdk::String)> {
        let storage_key = DataKey::UserMetadata(user);
        env.storage().persistent().get(&storage_key)
    }

    /// Admin-only: extend the TTL (time-to-live) of core persistent entries.
    /// Bumps ledger entry lifetimes by `ledgers` ledgers for TotalSupply,
    /// all stored Transaction entries, associated Balance entries (sender
    /// and recipient addresses found in transactions), and all UserMetadata
    /// entries found in transactions. Call periodically to prevent
    /// ledger eviction of persistent data.
    ///
    /// # Security
    /// Requires admin auth. Ledgers must be greater than zero.
    pub fn extend_ttl(env: Env, ledgers: u32) {
        Self::require_admin(&env);
        assert!(ledgers > 0, "ledgers must be greater than zero");
        let threshold = ledgers;

        // Extend TotalSupply entry
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::TotalSupply, threshold, threshold);

        // Collect unique addresses from all transactions and extend
        // their Balance and UserMetadata entries alongside the transactions.
        let count: u64 = env.storage().instance().get(&DataKey::TxCount).unwrap_or(0);
        for id in 1..=count {
            let tx_key = DataKey::Transaction(id);
            if env.storage().persistent().has(&tx_key) {
                // Extend the transaction entry itself
                env.storage()
                    .persistent()
                    .extend_ttl(&tx_key, threshold, threshold);

                // Read the transaction to find associated addresses
                if let Some(tx) = env
                    .storage()
                    .persistent()
                    .get::<DataKey, Transaction>(&tx_key)
                {
                    // Extend Balance for sender
                    let sender_bal_key = DataKey::Balance(tx.sender.clone());
                    if env.storage().persistent().has(&sender_bal_key) {
                        env.storage().persistent().extend_ttl(
                            &sender_bal_key,
                            threshold,
                            threshold,
                        );
                    }
                    // Extend Balance for recipient
                    let rec_bal_key = DataKey::Balance(tx.recipient.clone());
                    if env.storage().persistent().has(&rec_bal_key) {
                        env.storage()
                            .persistent()
                            .extend_ttl(&rec_bal_key, threshold, threshold);
                    }
                    // Extend UserMetadata for sender
                    let sender_meta_key = DataKey::UserMetadata(tx.sender.clone());
                    if env.storage().persistent().has(&sender_meta_key) {
                        env.storage().persistent().extend_ttl(
                            &sender_meta_key,
                            threshold,
                            threshold,
                        );
                    }
                    // Extend UserMetadata for recipient
                    let rec_meta_key = DataKey::UserMetadata(tx.recipient);
                    if env.storage().persistent().has(&rec_meta_key) {
                        env.storage()
                            .persistent()
                            .extend_ttl(&rec_meta_key, threshold, threshold);
                    }
                }
            }
        }

        env.events()
            .publish((Symbol::new(&env, "ttl_extended"),), ledgers);
    }

    /// Read a transaction by ID.
    pub fn get_transaction(env: Env, transaction_id: u64) -> Transaction {
        env.storage()
            .persistent()
            .get(&DataKey::Transaction(transaction_id))
            .expect("transaction not found")
    }

    /// Check whether a transaction with the given ID exists.
    /// Returns true if the transaction is stored, false otherwise.
    pub fn transaction_exists(env: Env, transaction_id: u64) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::Transaction(transaction_id))
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
    ///
    /// # Security
    /// The treasury address must not be the contract itself to prevent
    /// fee funds from becoming permanently locked.
    pub fn set_fee(env: Env, fee_bps: u32, treasury: Address) {
        Self::require_admin(&env);
        assert!(fee_bps <= 10_000, "fee basis points must not exceed 10_000");
        assert!(
            treasury != env.current_contract_address(),
            "treasury cannot be the contract itself"
        );

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

    // ── pagination & queries ────────────────────────────

    /// Return a page of transactions with offset-based pagination.
    /// `offset`: starting tx ID (1-based). `limit`: max results (capped at 50).
    /// Returns Vec<Transaction> for the requested page.
    pub fn get_transactions_page(
        env: Env,
        offset: u64,
        limit: u64,
    ) -> soroban_sdk::Vec<Transaction> {
        let count: u64 = env.storage().instance().get(&DataKey::TxCount).unwrap_or(0);
        let max_limit = 50u64;
        let effective_limit = if limit > max_limit { max_limit } else { limit };
        let start = if offset < 1 { 1 } else { offset };
        let end = if start + effective_limit - 1 > count {
            count + 1
        } else {
            start + effective_limit
        };

        let mut result = soroban_sdk::Vec::new(&env);
        for id in start..end {
            if let Some(tx) = env
                .storage()
                .persistent()
                .get::<DataKey, Transaction>(&DataKey::Transaction(id))
            {
                result.push_back(tx);
            }
        }
        result
    }

    /// Query all transaction IDs associated with a user address.
    /// Returns paginated Vec<u64> of transaction IDs where user is sender or recipient.
    pub fn query_user_transactions(
        env: Env,
        user: Address,
        limit: u64,
        offset: u64,
    ) -> soroban_sdk::Vec<u64> {
        let count: u64 = env.storage().instance().get(&DataKey::TxCount).unwrap_or(0);
        let max_limit = 50u64;
        let effective_limit = if limit > max_limit { max_limit } else { limit };

        let mut result = soroban_sdk::Vec::new(&env);
        let mut skipped: u64 = 0;
        for id in 1..=count {
            if let Some(tx) = env
                .storage()
                .persistent()
                .get::<DataKey, Transaction>(&DataKey::Transaction(id))
            {
                if tx.sender == user || tx.recipient == user {
                    if skipped < offset {
                        skipped += 1;
                        continue;
                    }
                    result.push_back(id);
                    if result.len() as u64 >= effective_limit {
                        break;
                    }
                }
            }
        }
        result
    }

    // ── batch operations ─────────────────────────────────

    /// Admin-only: deposit funds to multiple recipients in a single call.
    /// `recipients` and `amounts` must have matching lengths.
    /// Each deposit validated independently (min amounts, paused check).
    pub fn batch_deposit(
        env: Env,
        recipients: soroban_sdk::Vec<Address>,
        amounts: soroban_sdk::Vec<i128>,
    ) {
        Self::require_admin(&env);
        Self::require_not_paused(&env);
        assert!(
            recipients.len() == amounts.len(),
            "recipients and amounts length mismatch"
        );
        assert!(recipients.len() > 0, "must provide at least one recipient");

        let count = recipients.len();
        for i in 0..count {
            let recipient = recipients.get(i).expect("recipient access failed");
            let amount = amounts.get(i).expect("amount access failed");
            assert!(amount > 0, "batch deposit amount must be greater than zero");
            const MIN_DEPOSIT: i128 = 1_000_000;
            assert!(
                amount >= MIN_DEPOSIT,
                "amount below minimum deposit threshold"
            );

            let key = DataKey::Balance(recipient.clone());
            let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
            let new_balance = balance.checked_add(amount).expect("arithmetic overflow");
            env.storage().persistent().set(&key, &new_balance);

            Self::add_supply(&env, amount);
        }

        env.events()
            .publish((Symbol::new(&env, "batch_deposit"),), count);
    }

    // ── fee collection ───────────────────────────────────

    /// Admin-only: collect accumulated fees from treasury to a destination.
    /// Transfers entire treasury balance to `to` address.
    /// Only works when FeeConfig is set with a treasury address.
    pub fn collect_fees(env: Env, to: Address) -> i128 {
        Self::require_admin(&env);
        assert!(
            to != env.current_contract_address(),
            "cannot collect fees to contract itself"
        );

        let fee_config: FeeConfig = env
            .storage()
            .instance()
            .get(&DataKey::FeeConfig)
            .expect("fee configuration not set");

        let treasury_key = DataKey::Balance(fee_config.treasury.clone());
        let treasury_bal: i128 = env.storage().persistent().get(&treasury_key).unwrap_or(0);
        assert!(treasury_bal > 0, "no fees to collect");

        // Transfer to destination
        let to_key = DataKey::Balance(to.clone());
        let to_bal: i128 = env.storage().persistent().get(&to_key).unwrap_or(0);

        env.storage()
            .persistent()
            .set(&treasury_key, &0i128);
        env.storage().persistent().set(
            &to_key,
            &to_bal.checked_add(treasury_bal).expect("arithmetic overflow"),
        );

        env.events().publish(
            (Symbol::new(&env, "fees_collected"), fee_config.treasury),
            (to, treasury_bal),
        );

        treasury_bal
    }

    // ── admin escrow overrides ───────────────────────────

    /// Admin-only: force-release an escrowed transaction to the recipient.
    /// Bypasses sender auth and expiry checks for dispute resolution.
    pub fn admin_release_escrow(env: Env, transaction_id: u64) {
        Self::require_admin(&env);
        let key = DataKey::Transaction(transaction_id);
        let mut tx: Transaction = env
            .storage()
            .persistent()
            .get(&key)
            .expect("transaction not found");

        assert!(
            tx.status == TransactionStatus::Escrowed,
            "transaction not in escrow"
        );

        let (net_amount, fee_amount) = Self::compute_fee(&env, tx.amount);
        assert!(
            net_amount > 0,
            "fee exceeds escrow amount — recipient would receive zero"
        );

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
            (Symbol::new(&env, "admin_escrow_released"),),
            (transaction_id, tx.recipient),
        );
    }

    /// Admin-only: force-cancel an escrowed transaction and refund sender.
    /// Bypasses sender auth and expiry checks for dispute resolution.
    pub fn admin_cancel_escrow(env: Env, transaction_id: u64) {
        Self::require_admin(&env);
        let key = DataKey::Transaction(transaction_id);
        let mut tx: Transaction = env
            .storage()
            .persistent()
            .get(&key)
            .expect("transaction not found");

        assert!(
            tx.status == TransactionStatus::Escrowed,
            "transaction not in escrow"
        );

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
            (Symbol::new(&env, "admin_escrow_cancelled"),),
            (transaction_id, tx.amount),
        );
    }

    // ── recipient confirmation ───────────────────────────

    /// Recipient confirms they want to receive an escrowed transfer.
    /// Must be called before release_escrow when confirmation is enabled.
    pub fn confirm_escrow(env: Env, transaction_id: u64) {
        let key = DataKey::Transaction(transaction_id);
        let mut tx: Transaction = env
            .storage()
            .persistent()
            .get(&key)
            .expect("transaction not found");

        assert!(
            tx.status == TransactionStatus::Escrowed,
            "transaction not in escrow"
        );
        assert!(!tx.recipient_confirmed, "escrow already confirmed");

        tx.recipient.require_auth();
        tx.recipient_confirmed = true;
        env.storage().persistent().set(&key, &tx);

        env.events().publish(
            (Symbol::new(&env, "escrow_confirmed"), tx.recipient),
            transaction_id,
        );
    }

    /// Check if an escrowed transaction has been confirmed by the recipient.
    pub fn is_escrow_confirmed(env: Env, transaction_id: u64) -> bool {
        env.storage()
            .persistent()
            .get::<DataKey, Transaction>(&DataKey::Transaction(transaction_id))
            .map(|tx| tx.recipient_confirmed)
            .unwrap_or(false)
    }

    // ── upgrade tracking ─────────────────────────────────

    /// Admin-only: record a contract upgrade with new version.
    /// Tracks upgrade count, timestamp, and previous version.
    pub fn record_upgrade(env: Env, new_version: soroban_sdk::String) {
        Self::require_admin(&env);

        let current_version = Self::version(env.clone());
        let upgrade_data: (u32, u64, soroban_sdk::String) = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeHistory)
            .unwrap_or((0u32, 0u64, soroban_sdk::String::from_str(&env, "0.0.0")));

        let new_count = upgrade_data.0 + 1;
        let now = env.ledger().timestamp();
        let record = (new_count, now, current_version);
        env.storage().instance().set(&DataKey::UpgradeHistory, &record);

        env.events().publish(
            (Symbol::new(&env, "contract_upgraded"),),
            (new_count, new_version),
        );
    }

    /// Return upgrade history: (count, last_timestamp, previous_version).
    pub fn get_upgrade_info(
        env: Env,
    ) -> (u32, u64, soroban_sdk::String) {
        env.storage()
            .instance()
            .get(&DataKey::UpgradeHistory)
            .unwrap_or((0u32, 0u64, soroban_sdk::String::from_str(&env, "0.0.0")))
    }

    // ── daily volume limit ───────────────────────────────

    /// Admin-only: set the daily transfer volume limit per address.
    /// Set to 0 to disable (default).
    pub fn set_daily_limit(env: Env, limit: i128) {
        Self::require_admin(&env);
        env.storage().instance().set(&DataKey::DailyLimit, &limit);

        env.events()
            .publish((Symbol::new(&env, "daily_limit_updated"),), limit);
    }

    /// Read the current daily transfer volume limit.
    pub fn get_daily_limit(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::DailyLimit)
            .unwrap_or(0)
    }

    // ── multi-sig admin ──────────────────────────────────

    /// Admin-only: add an address to the admin set (multi-sig).
    /// First call enables multi-sig mode. Single admin remains supported.
    pub fn add_admin(env: Env, new_admin: Address) {
        Self::require_admin(&env);
        assert!(
            new_admin != env.current_contract_address(),
            "admin cannot be the contract itself"
        );

        let mut admins: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AdminSet)
            .unwrap_or_else(|| {
                // Initialize with current admin
                let current_admin: Address = env
                    .storage()
                    .instance()
                    .get(&DataKey::Admin)
                    .expect("admin not initialized");
                soroban_sdk::vec![&env, current_admin]
            });

        // Check for duplicates
        for admin in admins.iter() {
            assert!(admin != new_admin, "admin already in set");
        }

        admins.push_back(new_admin.clone());
        env.storage().instance().set(&DataKey::AdminSet, &admins);

        // Set default threshold to 1 if not set
        if !env.storage().instance().has(&DataKey::ApprovalThreshold) {
            env.storage()
                .instance()
                .set(&DataKey::ApprovalThreshold, &1u32);
        }

        env.events()
            .publish((Symbol::new(&env, "admin_added"),), new_admin);
    }

    /// Admin-only: remove an address from the admin set.
    pub fn remove_admin(env: Env, admin_to_remove: Address) {
        Self::require_admin(&env);

        let admins: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AdminSet)
            .expect("admin set not initialized");

        let len_before = admins.len();
        let mut new_admins = soroban_sdk::Vec::new(&env);
        for admin in admins.iter() {
            if admin != admin_to_remove {
                new_admins.push_back(admin);
            }
        }
        assert!(
            new_admins.len() < len_before,
            "admin not found in set"
        );
        assert!(new_admins.len() > 0, "cannot remove last admin");

        env.storage().instance().set(&DataKey::AdminSet, &new_admins);

        // Update primary admin if it was removed
        let primary: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or(admin_to_remove.clone());
        if primary == admin_to_remove {
            let first = new_admins.first().expect("no admins left");
            env.storage().instance().set(&DataKey::Admin, &first);
        }

        env.events()
            .publish((Symbol::new(&env, "admin_removed"),), admin_to_remove);
    }

    /// Admin-only: set the multi-sig approval threshold (1 to admin_count).
    pub fn set_approval_threshold(env: Env, threshold: u32) {
        Self::require_admin(&env);
        assert!(threshold > 0, "threshold must be at least 1");

        let admins: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::AdminSet)
            .unwrap_or_else(|| {
                soroban_sdk::vec![&env]
            });
        assert!(
            threshold as u32 <= admins.len(),
            "threshold cannot exceed admin count"
        );

        env.storage()
            .instance()
            .set(&DataKey::ApprovalThreshold, &threshold);

        env.events()
            .publish((Symbol::new(&env, "threshold_updated"),), threshold);
    }

    /// Read the current multi-sig admin set.
    pub fn get_admin_set(env: Env) -> soroban_sdk::Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::AdminSet)
            .unwrap_or_else(|| soroban_sdk::vec![&env])
    }

    /// Read the current approval threshold.
    pub fn get_approval_threshold(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::ApprovalThreshold)
            .unwrap_or(1)
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

    /// Enforce daily transfer volume limit per address.
    /// Checks and updates the running daily total for the address.
    fn check_daily_volume(env: &Env, addr: &Address, amount: i128) {
        let daily_limit: i128 = env
            .storage()
            .instance()
            .get(&DataKey::DailyLimit)
            .unwrap_or(0);
        if daily_limit == 0 {
            return;
        }

        let ledger_day = u64::from(env.ledger().sequence()) / 17280; // ~24h of ledgers
        let volume_key = DataKey::DailyVolume(addr.clone());
        let (stored_day, current_volume): (u64, i128) = env
            .storage()
            .persistent()
            .get(&volume_key)
            .unwrap_or((0u64, 0i128));

        let effective_volume = if stored_day == ledger_day {
            current_volume
        } else {
            0i128
        };

        assert!(
            effective_volume + amount <= daily_limit,
            "daily transfer volume limit exceeded"
        );

        env.storage().persistent().set(
            &volume_key,
            &(ledger_day, effective_volume + amount),
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

    fn add_supply(env: &Env, amount: i128) {
        let supply: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0);
        env.storage().persistent().set(
            &DataKey::TotalSupply,
            &supply.checked_add(amount).expect("arithmetic overflow"),
        );
    }

    fn sub_supply(env: &Env, amount: i128) {
        let supply: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0);
        env.storage().persistent().set(
            &DataKey::TotalSupply,
            &supply.checked_sub(amount).expect("arithmetic overflow"),
        );
    }
}
