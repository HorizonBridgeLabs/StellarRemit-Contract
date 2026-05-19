#![cfg(test)]

use soroban_sdk::{
    testutils::{Events, Address as AddressTestUtils},
    vec, Symbol, IntoVal, Env, Address,
};
use stellarremit_contract::{RemittanceContract, RemittanceContractClient};

#[test]
fn test_send_emits_transfer_created_event() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, RemittanceContract);
    let client = RemittanceContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);

    client.init(&admin);
    client.deposit(&sender, &10_000_000);

    let tx_id = client.send(&sender, &recipient, &5_000_000);

    // Check that transfer_created event was published
    let events = env.events().all();
    let transfer_created = events.iter().find(|(_, topics, _)| {
        *topics
            == vec![
                &env,
                Symbol::new(&env, "transfer_created").into_val(&env),
                sender.clone().into_val(&env),
            ]
    });

    assert!(transfer_created.is_some(), "transfer_created event not emitted");

    let (_, _, data) = transfer_created.unwrap();
    let (emitted_id, emitted_amount): (u64, i128) = data.into_val(&env);
    assert_eq!(emitted_id, tx_id);
    assert_eq!(emitted_amount, 5_000_000);

    // Check that transfer_completed event was published
    let transfer_completed = events.iter().find(|(_, topics, _)| {
        *topics
            == vec![
                &env,
                Symbol::new(&env, "transfer_completed").into_val(&env),
                sender.clone().into_val(&env),
            ]
    });

    assert!(transfer_completed.is_some(), "transfer_completed event not emitted");

    let (_, _, data2) = transfer_completed.unwrap();
    let (emitted_id2, emitted_recipient): (u64, Address) = data2.into_val(&env);
    assert_eq!(emitted_id2, tx_id);
    assert_eq!(emitted_recipient, recipient);
}