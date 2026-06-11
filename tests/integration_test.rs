#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Env, IntoVal, Symbol,
};
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

    // Advance time past rate limit cooldown
    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: env.ledger().timestamp() + 301,
        protocol_version: env.ledger().protocol_version(),
        sequence_number: env.ledger().sequence() + 50,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 10,
        min_persistent_entry_ttl: 10,
        max_entry_ttl: 3110400,
    });

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
fn test_transfer_admin() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    client.init(&admin);

    assert_eq!(client.get_admin(), admin);

    client.transfer_admin(&new_admin);

    assert_eq!(client.get_admin(), new_admin);

    // Verify admin_transferred event was emitted
    let events = env.events().all();
    let admin_transferred = events.iter().find(|(_, topics, _)| {
        *topics
            == soroban_sdk::vec![
                &env,
                soroban_sdk::Symbol::new(&env, "admin_transferred").into_val(&env),
                admin.clone().into_val(&env),
            ]
    });
    assert!(
        admin_transferred.is_some(),
        "admin_transferred event not emitted"
    );

    let (_, _, data) = admin_transferred.unwrap();
    let emitted_admin: Address = data.into_val(&env);
    assert_eq!(emitted_admin, new_admin);
}

#[test]
#[should_panic(expected = "transaction not found")]
fn test_get_transaction_not_found_panics() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    client.init(&admin);
    client.get_transaction(&999);
}

// ── cancel_escrow tests ─────────────────────────────────

#[test]
fn test_cancel_escrow_refunds_sender() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);

    let tx_id = client.escrow_funds(&sender, &recipient, &400_000, &0);
    assert_eq!(client.balance(&sender), 600_000);
    assert_eq!(client.balance(&recipient), 0);

    client.cancel_escrow(&tx_id);

    // Sender gets refund
    assert_eq!(client.balance(&sender), 1_000_000);
    assert_eq!(client.balance(&recipient), 0);

    let tx = client.get_transaction(&tx_id);
    assert_eq!(tx.status, TransactionStatus::Cancelled);
}

#[test]
fn test_cancel_escrow_emits_event() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);

    let tx_id = client.escrow_funds(&sender, &recipient, &400_000, &0);
    client.cancel_escrow(&tx_id);

    let events = env.events().all();
    let cancelled = events.iter().find(|(_, topics, _)| {
        *topics
            == soroban_sdk::vec![
                &env,
                Symbol::new(&env, "escrow_cancelled").into_val(&env),
                sender.clone().into_val(&env),
            ]
    });
    assert!(cancelled.is_some(), "escrow_cancelled event not emitted");
}

#[test]
#[should_panic(expected = "not in escrow")]
fn test_cancel_escrow_fails_on_released() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);

    let tx_id = client.escrow_funds(&sender, &recipient, &400_000, &0);
    client.release_escrow(&tx_id);
    client.cancel_escrow(&tx_id); // should panic
}

#[test]
#[should_panic(expected = "escrow not yet expired")]
fn test_cancel_escrow_fails_before_expiry() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);

    let tx_id = client.escrow_funds(&sender, &recipient, &400_000, &10);
    // expiry has NOT passed — cancellation should panic
    client.cancel_escrow(&tx_id);
}

// ── pause / unpause tests ───────────────────────────────

#[test]
fn test_pause_and_unpause() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    client.init(&admin);

    assert!(!client.is_paused());

    client.pause();
    assert!(client.is_paused());

    client.unpause();
    assert!(!client.is_paused());
}

#[test]
#[should_panic(expected = "contract is paused")]
fn test_paused_prevents_deposit() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin);

    client.pause();
    client.deposit(&user, &1_000_000); // should panic
}

#[test]
#[should_panic(expected = "contract is paused")]
fn test_paused_prevents_send() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);

    client.pause();
    client.send(&sender, &recipient, &100_000); // should panic
}

#[test]
#[should_panic(expected = "contract is paused")]
fn test_paused_prevents_escrow() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);

    client.pause();
    client.escrow_funds(&sender, &recipient, &100_000, &0); // should panic
}

#[test]
fn test_release_still_works_when_paused() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);

    let tx_id = client.escrow_funds(&sender, &recipient, &400_000, &0);

    client.pause();
    assert!(client.is_paused());

    // Release should still work even while paused
    client.release_escrow(&tx_id);
    assert_eq!(client.balance(&recipient), 400_000);
}

#[test]
#[should_panic(expected = "contract already paused")]
fn test_double_pause_panics() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    client.init(&admin);

    client.pause();
    client.pause(); // should panic
}

#[test]
#[should_panic(expected = "contract not paused")]
fn test_unpause_when_not_paused_panics() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    client.init(&admin);

    client.unpause(); // should panic
}

// ── sender ≠ recipient tests ────────────────────────────

#[test]
#[should_panic(expected = "cannot send to yourself")]
fn test_send_to_self_fails() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin);
    client.deposit(&user, &1_000_000);
    client.send(&user, &user, &100_000); // should panic
}

#[test]
#[should_panic(expected = "cannot escrow to yourself")]
fn test_escrow_to_self_fails() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin);
    client.deposit(&user, &1_000_000);
    client.escrow_funds(&user, &user, &100_000, &0); // should panic
}

// ── rate limit tests ────────────────────────────────────

#[test]
fn test_rate_limit_allows_first_operation() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &2_000_000);

    // First operation should succeed — no prior timestamp
    let tx_id = client.send(&sender, &recipient, &500_000);
    assert!(tx_id > 0);
}

#[test]
#[should_panic(expected = "rate limit exceeded")]
fn test_rate_limit_blocks_rapid_operations() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &2_000_000);

    client.send(&sender, &recipient, &500_000);
    // Immediate second send should fail — rate limited
    client.send(&sender, &recipient, &500_000);
}

#[test]
fn test_rate_limit_allows_after_cooldown() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &2_000_000);

    client.send(&sender, &recipient, &500_000);

    // Advance time past 300s cooldown
    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: env.ledger().timestamp() + 301,
        protocol_version: env.ledger().protocol_version(),
        sequence_number: env.ledger().sequence() + 50,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 10,
        min_persistent_entry_ttl: 10,
        max_entry_ttl: 3110400,
    });

    // Should succeed after cooldown
    let tx_id = client.send(&sender, &recipient, &200_000);
    assert!(tx_id > 0);
}

// ── withdraw tests ──────────────────────────────────────

#[test]
fn test_withdraw_moves_funds() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin);

    // Deposit into admin like a fee collection
    client.deposit(&admin, &5_000_000);

    client.withdraw(&admin, &treasury, &1_000_000);

    assert_eq!(client.balance(&admin), 4_000_000);
    assert_eq!(client.balance(&treasury), 1_000_000);
}

#[test]
#[should_panic(expected = "rate limit exceeded")]
fn test_rate_limit_blocks_rapid_escrow() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &2_000_000);

    client.escrow_funds(&sender, &recipient, &500_000, &0);
    // Immediate second escrow should fail — rate limited
    client.escrow_funds(&sender, &recipient, &500_000, &0);
}

#[test]
#[should_panic(expected = "insufficient balance")]
fn test_withdraw_insufficient_balance() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin);

    client.withdraw(&admin, &treasury, &1_000_000); // no balance — should panic
}

// ── configurable rate limit tests ───────────────────────

#[test]
fn test_default_rate_limit_is_300() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    client.init(&admin);

    assert_eq!(client.get_rate_limit(), 300);
}

#[test]
fn test_set_rate_limit_changes_cooldown() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &2_000_000);

    // Reduce cooldown to 10 seconds
    client.set_rate_limit(&10);
    assert_eq!(client.get_rate_limit(), 10);

    // First operation
    client.send(&sender, &recipient, &500_000);

    // Advance time just 11 seconds — should pass with shorter cooldown
    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: env.ledger().timestamp() + 11,
        protocol_version: env.ledger().protocol_version(),
        sequence_number: env.ledger().sequence() + 10,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 10,
        min_persistent_entry_ttl: 10,
        max_entry_ttl: 3110400,
    });

    let tx_id = client.send(&sender, &recipient, &200_000);
    assert!(tx_id > 0);
}

#[test]
fn test_disable_rate_limit_with_zero() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &2_000_000);

    // Disable rate limiting
    client.set_rate_limit(&0);
    assert_eq!(client.get_rate_limit(), 0);

    // Back-to-back sends should work without cooldown
    client.send(&sender, &recipient, &500_000);
    client.send(&sender, &recipient, &200_000);
}

#[test]
#[should_panic(expected = "rate limit exceeded")]
fn test_custom_rate_limit_still_enforced() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &2_000_000);

    // Set a longer cooldown
    client.set_rate_limit(&600);

    client.send(&sender, &recipient, &500_000);
    // Immediate second send should still fail
    client.send(&sender, &recipient, &200_000);
}
