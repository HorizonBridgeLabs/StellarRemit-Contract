#![no_std]

mod types;

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};
use types::{DataKey, Transaction, TransactionStatus};

#[contract]
pub struct RemittanceContract;

#[contractimpl]
impl RemittanceContract {
    /// Initialize contract with an admin address.
    pub fn init(env: Env, admin: Address) {
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TxCount, &0u64);
    }

    /// Deposit funds into sender's on-chain balance.
    pub fn deposit(env: Env, sender: Address, amount: i128) {
        sender.require_auth();
        assert!(amount > 0, "amount must be positive");
        let key = DataKey::Balance(sender.clone());
        let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(balance + amount));
    }

    /// Transfer funds from sender to recipient immediately.
    pub fn send(env: Env, sender: Address, recipient: Address, amount: i128) -> u64 {
        sender.require_auth();
        assert!(amount > 0, "amount must be positive");

        let sender_key = DataKey::Balance(sender.clone());
        let sender_bal: i128 = env.storage().persistent().get(&sender_key).unwrap_or(0);
        assert!(sender_bal >= amount, "insufficient balance");

        let recipient_key = DataKey::Balance(recipient.clone());
        let recipient_bal: i128 = env.storage().persistent().get(&recipient_key).unwrap_or(0);

        env.storage().persistent().set(&sender_key, &(sender_bal - amount));
        env.storage().persistent().set(&recipient_key, &(recipient_bal + amount));

        let id = Self::next_id(&env);
        let tx = Transaction {
            id,
            sender: sender.clone(),
            recipient: recipient.clone(),
            amount,
            status: TransactionStatus::Completed,
        };
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
    pub fn escrow_funds(env: Env, sender: Address, recipient: Address, amount: i128) -> u64 {
        sender.require_auth();
        assert!(amount > 0, "amount must be positive");

        let sender_key = DataKey::Balance(sender.clone());
        let sender_bal: i128 = env.storage().persistent().get(&sender_key).unwrap_or(0);
        assert!(sender_bal >= amount, "insufficient balance");

        env.storage().persistent().set(&sender_key, &(sender_bal - amount));

        let id = Self::next_id(&env);
        let tx = Transaction {
            id,
            sender: sender.clone(),
            recipient,
            amount,
            status: TransactionStatus::Escrowed,
        };
        env.storage().persistent().set(&DataKey::Transaction(id), &tx);

        env.events().publish(
            (Symbol::new(&env, "transfer_created"), sender),
            (id, amount),
        );
        id
    }

    /// Release escrowed funds to recipient.
    pub fn release_escrow(env: Env, transaction_id: u64) {
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

        tx.sender.require_auth();

        let recipient_key = DataKey::Balance(tx.recipient.clone());
        let recipient_bal: i128 = env.storage().persistent().get(&recipient_key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&recipient_key, &(recipient_bal + tx.amount));

        tx.status = TransactionStatus::Released;
        env.storage().persistent().set(&key, &tx);

        env.events().publish(
            (Symbol::new(&env, "transfer_completed"), tx.sender),
            (transaction_id, tx.recipient),
        );
    }

    /// Read a transaction by ID.
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

    // ── helpers ──────────────────────────────────────────────────────────────

    fn next_id(env: &Env) -> u64 {
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
