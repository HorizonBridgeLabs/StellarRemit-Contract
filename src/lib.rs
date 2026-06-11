#![no_std]

mod types;

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};
use types::{DataKey, Transaction, TransactionStatus};

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
        // Admin and TxCount use instance() storage: they are contract-scoped singletons
        // that share the contract instance's ledger entry and are evicted together.
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TxCount, &0u64);
    }

    /// Deposit funds into sender's on-chain balance.!!!!!!
    /// Returns the new balance after deposit.
    pub fn deposit(env: Env, sender: Address, amount: i128) -> i128 {
        sender.require_auth();
        
        // Enhanced validation
        assert!(amount > 0, "Deposit amount must be greater than zero");
        const MIN_DEPOSIT: i128 = 1_000_000; // Minimum deposit amount (0.000001 XLM equivalent)
        assert!(amount >= MIN_DEPOSIT, "amount below minimum deposit threshold");
        
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

        // Balance keys use persistent() storage so user funds survive instance eviction.
        let sender_key = DataKey::Balance(sender.clone());
        // Balance uses persistent storage — survives ledger expiry
        let sender_bal: i128 = env.storage().persistent().get(&sender_key).unwrap_or(0);
        assert!(sender_bal >= amount, "insufficient balance");

        let recipient_key = DataKey::Balance(recipient.clone());
        let recipient_bal: i128 = env.storage().persistent().get(&recipient_key).unwrap_or(0);

        env.storage()
            .persistent()
            .set(&sender_key, &sender_bal.checked_sub(amount).expect("arithmetic overflow"));
        env.storage()
            .persistent()
            .set(&recipient_key, &recipient_bal.checked_add(amount).expect("arithmetic overflow"));

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
        env.storage().persistent().set(&DataKey::Transaction(id), &tx);

        env.events().publish(
            (Symbol::new(&env, "transfer_created"), sender.clone()),
            (id, amount),
        );
        env.events().publish(
            (Symbol::new(&env, "transfer_completed"), sender),
            (id, recipient),
        );
        id
    }

    /// Lock funds in escrow pending release.
    /// `expiry_ledgers`: number of ledgers until escrow expires (0 = no expiry).
    pub fn escrow_funds(env: Env, sender: Address, recipient: Address, amount: i128, expiry_ledgers: u32) -> u64 {
        sender.require_auth();
        assert!(amount > 0, "Escrow amount must be greater than zero");

        // Balance uses persistent storage — survives ledger expiry
        let sender_key = DataKey::Balance(sender.clone());
        let sender_bal: i128 = env.storage().persistent().get(&sender_key).unwrap_or(0);
        assert!(sender_bal >= amount, "insufficient balance");

        env.storage()
            .persistent()
            .set(&sender_key, &sender_bal.checked_sub(amount).expect("arithmetic overflow"));

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
        env.storage().persistent().set(&DataKey::Transaction(id), &tx);

        env.events().publish(
            (Symbol::new(&env, "transfer_created"), sender),
            (id, amount),
        );
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

        assert!(
            tx.status == TransactionStatus::Escrowed,
            "not in escrow"
        );

        // Expiry check: if expires_at is set, current ledger must not exceed it
        if tx.expires_at > 0 {
            assert!(
                u64::from(env.ledger().sequence()) <= tx.expires_at,
                "escrow expired"
            );
        }

        tx.sender.require_auth();

        // Balance uses persistent storage — survives ledger expiry
        let recipient_key = DataKey::Balance(tx.recipient.clone());
        let recipient_bal: i128 = env.storage().persistent().get(&recipient_key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&recipient_key, &recipient_bal.checked_add(tx.amount).expect("arithmetic overflow"));

        tx.status = TransactionStatus::Released;
        env.storage().persistent().set(&key, &tx);

        env.events().publish(
            (Symbol::new(&env, "transfer_completed"), tx.sender),
            (transaction_id, tx.recipient),
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

    // ──__________________ helpers ________________________

    fn next_id(env: &Env) -> u64 {
        // TxCount uses instance() storage: it is a contract-scoped counter that
        // lives and expires with the contract instance.
        let count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TxCount)
            .unwrap_or(0);
        let next = count + 1;
        env.storage().instance().set(&DataKey::TxCount, &next);
        next
    }
}
