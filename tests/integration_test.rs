#![cfg(test)]

use soroban_sdk::{testutils::{Address as _, Events as _, Ledger as _}, Address, Env, IntoVal, Symbol};
use stellarremit_contract::{RemittanceContract, RemittanceContractClient, TransactionStatus};

fn setup() -> (Env, RemittanceContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceContract);
    let client = RemittanceContractClient::new(&env, &contract_id);
    (env, client)
}

#[test]
fn test_init() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    client.init(&admin);
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_init_twice_panics() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    client.init(&admin);
    client.init(&admin); // should panic
}

#[test]
fn test_get_admin() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    client.init(&admin);
    assert_eq!(client.get_admin(), admin);
}

#[test]
fn test_deposit_and_balance() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin);
    client.deposit(&user, &1_000_000);
    assert_eq!(client.balance(&user), 1_000_000);
}

#[test]
fn test_successful_send() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);
    let tx_id = client.send(&sender, &recipient, &200_000);
    assert_eq!(client.balance(&sender), 800_000);
    assert_eq!(client.balance(&recipient), 200_000);
    let tx = client.get_transaction(&tx_id);
    assert_eq!(tx.amount, 200_000);
    assert_eq!(tx.status, TransactionStatus::Completed);
}

#[test]
#[should_panic(expected = "insufficient balance")]
fn test_send_insufficient_balance() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);
    client.send(&sender, &recipient, &5_000_000); // should panic
}

#[test]
#[should_panic(expected = "insufficient balance")]
fn test_escrow_insufficient_balance() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);
    client.escrow_funds(&sender, &recipient, &5_000_000, &0); // should panic
}

#[test]
fn test_escrow_and_release() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);
    let tx_id = client.escrow_funds(&sender, &recipient, &400_000, &0);
    // funds deducted from sender, not yet at recipient
    assert_eq!(client.balance(&sender), 600_000);
    assert_eq!(client.balance(&recipient), 0);
    client.release_escrow(&tx_id);
    assert_eq!(client.balance(&recipient), 400_000);
    let tx = client.get_transaction(&tx_id);
    assert_eq!(tx.status, TransactionStatus::Released);
}

#[test]
fn test_tx_count() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &3_000_000);

    assert_eq!(client.tx_count(), 0);

    let first_tx_id = client.send(&sender, &recipient, &1_000_000);
    assert_eq!(first_tx_id, 1);
    assert_eq!(client.tx_count(), 1);

    let second_tx_id = client.escrow_funds(&sender, &recipient, &1_000_000, &0);
    assert_eq!(second_tx_id, 2);
    assert_eq!(client.tx_count(), 2);
}

#[test]
#[should_panic(expected = "not in escrow")]
fn test_double_release_fails() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);
    let tx_id = client.escrow_funds(&sender, &recipient, &400_000, &0);
    client.release_escrow(&tx_id);
    client.release_escrow(&tx_id); // should panic
}

#[test]
fn test_escrow_funds_emits_transfer_created_event() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);

    let tx_id = client.escrow_funds(&sender, &recipient, &400_000, &0);

    // deposit emits 1 event; escrow_funds emits 1 event — check the last one
    let events = env.events().all();
    assert_eq!(events.len(), 2);
    assert_eq!(
        events,
        soroban_sdk::vec![
            &env,
            (
                client.address.clone(),
                (Symbol::new(&env, "deposit"), sender.clone()).into_val(&env),
                (1_000_000i128, 1_000_000i128).into_val(&env),
            ),
            (
                client.address.clone(),
                (Symbol::new(&env, "transfer_created"), sender.clone()).into_val(&env),
                (tx_id, 400_000i128).into_val(&env),
            ),
        ]
    );
}

#[test]
fn test_release_escrow_emits_transfer_completed_event() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);

    let tx_id = client.escrow_funds(&sender, &recipient, &400_000, &0);
    client.release_escrow(&tx_id);

    let events = env.events().all();
    assert_eq!(events.len(), 3);
    assert_eq!(
        events,
        soroban_sdk::vec![
            &env,
            (
                client.address.clone(),
                (Symbol::new(&env, "deposit"), sender.clone()).into_val(&env),
                (1_000_000i128, 1_000_000i128).into_val(&env),
            ),
            (
                client.address.clone(),
                (Symbol::new(&env, "transfer_created"), sender.clone()).into_val(&env),
                (tx_id, 400_000i128).into_val(&env),
            ),
            (
                client.address.clone(),
                (Symbol::new(&env, "transfer_completed"), sender.clone()).into_val(&env),
                (tx_id, recipient.clone()).into_val(&env),
            ),
        ]
    );
}


#[test]
#[should_panic(expected = "escrow expired")]
fn test_escrow_expiry_prevents_release() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);

    // Create escrow that expires after 10 ledgers
    let tx_id = client.escrow_funds(&sender, &recipient, &400_000, &10);

    // Advance ledger sequence past expiry
    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: env.ledger().timestamp(),
        protocol_version: env.ledger().protocol_version(),
        sequence_number: env.ledger().sequence() + 11,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 10,
        min_persistent_entry_ttl: 10,
        max_entry_ttl: 3110400,
    });

    // Should panic: escrow expired
    client.release_escrow(&tx_id);
}

#[test]
#[should_panic(expected = "Send amount must be greater than zero")]
fn test_send_zero_amount_fails() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.send(&sender, &recipient, &0);
}

#[test]
#[should_panic(expected = "Deposit amount must be greater than zero")]
fn test_deposit_zero_amount_fails() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin);
    client.deposit(&user, &0);
}

#[test]
#[should_panic(expected = "Escrow amount must be greater than zero")]
fn test_escrow_zero_amount_fails() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.escrow_funds(&sender, &recipient, &0, &0);
}

#[test]
#[should_panic(expected = "transaction not found")]
fn test_get_transaction_not_found_panics() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    client.init(&admin);
    client.get_transaction(&999);
}
