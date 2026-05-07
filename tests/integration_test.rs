#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};
use stellarremit_contract::{RemittanceContract, RemittanceContractClient};

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
    client.deposit(&user, &1000);
    assert_eq!(client.balance(&user), 1000);
}

#[test]
fn test_successful_send() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &500);
    let tx_id = client.send(&sender, &recipient, &200);
    assert_eq!(client.balance(&sender), 300);
    assert_eq!(client.balance(&recipient), 200);
    let tx = client.get_transaction(&tx_id);
    assert_eq!(tx.amount, 200);
}

#[test]
#[should_panic(expected = "insufficient balance")]
fn test_send_insufficient_balance() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &100);
    client.send(&sender, &recipient, &500); // should panic
}

#[test]
fn test_escrow_and_release() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1000);
    let tx_id = client.escrow_funds(&sender, &recipient, &400);
    // funds deducted from sender, not yet at recipient
    assert_eq!(client.balance(&sender), 600);
    assert_eq!(client.balance(&recipient), 0);
    client.release_escrow(&tx_id);
    assert_eq!(client.balance(&recipient), 400);
}

#[test]
fn test_tx_count() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &10);

    assert_eq!(client.tx_count(), 0);

    let first_tx_id = client.send(&sender, &recipient, &1);
    assert_eq!(first_tx_id, 1);
    assert_eq!(client.tx_count(), 1);

    let second_tx_id = client.escrow_funds(&sender, &recipient, &1);
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
    client.deposit(&sender, &1000);
    let tx_id = client.escrow_funds(&sender, &recipient, &400);
    client.release_escrow(&tx_id);
    client.release_escrow(&tx_id); // should panic
}
