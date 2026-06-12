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

// ── fee tests ───────────────────────────────────────────

#[test]
fn test_default_fee_is_zero() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    client.init(&admin);

    let fee = client.get_fee();
    assert_eq!(fee.fee_bps, 0);
}

#[test]
fn test_set_and_get_fee() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin);

    client.set_fee(&250, &treasury); // 2.5%

    let fee = client.get_fee();
    assert_eq!(fee.fee_bps, 250);
    assert_eq!(fee.treasury, treasury);
}

#[test]
fn test_fee_deducted_on_send() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);

    // 10% fee (1000 bps)
    client.set_fee(&1000, &treasury);

    // Send 200k: fee=20k, recipient gets 180k
    client.send(&sender, &recipient, &200_000);

    assert_eq!(client.balance(&sender), 800_000);
    assert_eq!(client.balance(&recipient), 180_000);
    assert_eq!(client.balance(&treasury), 20_000);
}

#[test]
fn test_fee_deducted_on_escrow_release() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);

    // 5% fee (500 bps)
    client.set_fee(&500, &treasury);

    let tx_id = client.escrow_funds(&sender, &recipient, &400_000, &0);
    assert_eq!(client.balance(&sender), 600_000);

    client.release_escrow(&tx_id);

    // fee=20k, recipient gets 380k
    assert_eq!(client.balance(&recipient), 380_000);
    assert_eq!(client.balance(&treasury), 20_000);
}

#[test]
fn test_cancel_escrow_refunds_full_no_fee() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);

    // 10% fee — but cancel should NOT charge fee
    client.set_fee(&1000, &treasury);

    let tx_id = client.escrow_funds(&sender, &recipient, &400_000, &0);
    client.cancel_escrow(&tx_id);

    // Sender gets full refund, treasury gets nothing
    assert_eq!(client.balance(&sender), 1_000_000);
    assert_eq!(client.balance(&treasury), 0);
}

#[test]
#[should_panic(expected = "fee exceeds transfer amount")]
fn test_fee_exceeds_amount_panics() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);

    // 100% fee (10000 bps) — recipient gets 0
    client.set_fee(&10000, &treasury);

    client.send(&sender, &recipient, &100_000); // should panic
}

#[test]
#[should_panic(expected = "fee basis points must not exceed 10_000")]
fn test_set_fee_exceeds_max_panics() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin);

    client.set_fee(&10001, &treasury); // should panic
}

// ── total supply tests ──────────────────────────────────

#[test]
fn test_total_supply_starts_at_zero() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    client.init(&admin);

    assert_eq!(client.total_supply(), 0);
}

#[test]
fn test_total_supply_tracks_deposits() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin);

    client.deposit(&user, &1_000_000);
    assert_eq!(client.total_supply(), 1_000_000);

    client.deposit(&user, &2_000_000);
    assert_eq!(client.total_supply(), 3_000_000);
}

#[test]
fn test_total_supply_decreases_on_withdraw() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin);

    client.deposit(&admin, &5_000_000);
    assert_eq!(client.total_supply(), 5_000_000);

    client.withdraw(&admin, &treasury, &1_000_000);
    assert_eq!(client.total_supply(), 4_000_000);
}

// ── TTL extension tests ─────────────────────────────────

#[test]
fn test_extend_ttl_succeeds() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);
    client.send(&sender, &recipient, &200_000);

    // Extend all persistent entries by 5000 ledgers
    client.extend_ttl(&5000);

    // Verify the event was emitted
    let events = env.events().all();
    let ttl_event = events.iter().find(|(_, topics, _)| {
        *topics == soroban_sdk::vec![&env, Symbol::new(&env, "ttl_extended").into_val(&env),]
    });
    assert!(ttl_event.is_some(), "ttl_extended event not emitted");
}

// ── user metadata tests ────────────────────────────────

#[test]
fn test_set_and_get_user_metadata() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin);

    let key = soroban_sdk::String::from_str(&env, "kyc_status");
    let value = soroban_sdk::String::from_str(&env, "verified");
    client.set_user_metadata(&user, &key, &value);

    let result = client.get_user_metadata(&user);
    assert!(result.is_some());
    let (stored_key, stored_value) = result.unwrap();
    assert_eq!(stored_key, key);
    assert_eq!(stored_value, value);
}

#[test]
fn test_get_user_metadata_returns_none_for_new_user() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin);

    let result = client.get_user_metadata(&user);
    assert!(result.is_none());
}

#[test]
fn test_version_returns_string() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    client.init(&admin);

    let version = client.version();
    assert!(version.len() > 0);
}

#[test]
fn test_stats_returns_aggregate_data() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin);
    client.deposit(&user, &5_000_000);

    let (tx_count, supply, paused, cooldown) = client.stats();
    assert_eq!(tx_count, 0);
    assert_eq!(supply, 5_000_000);
    assert!(!paused);
    assert_eq!(cooldown, 300);
}

#[test]
fn test_stats_reflects_pause_state() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    client.init(&admin);

    let (_, _, paused, _) = client.stats();
    assert!(!paused);

    client.pause();
    let (_, _, paused, _) = client.stats();
    assert!(paused);
}

// ── edge case tests ─────────────────────────────────────

#[test]
fn test_send_exact_balance_succeeds() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);

    // Send exact balance
    let tx_id = client.send(&sender, &recipient, &1_000_000);
    assert!(tx_id > 0);
    assert_eq!(client.balance(&sender), 0);
    assert_eq!(client.balance(&recipient), 1_000_000);
}

#[test]
fn test_deposit_minimum_amount_succeeds() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin);

    client.deposit(&user, &1_000_000);
    assert_eq!(client.balance(&user), 1_000_000);
}

#[test]
fn test_set_fee_emits_event() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin);

    client.set_fee(&500, &treasury);

    let events = env.events().all();
    let fee_event = events.iter().find(|(_, topics, _)| {
        *topics == soroban_sdk::vec![&env, Symbol::new(&env, "fee_updated").into_val(&env),]
    });
    assert!(fee_event.is_some(), "fee_updated event not emitted");
}

#[test]
fn test_stats_reflects_config_changes() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    client.init(&admin);

    client.set_rate_limit(&120);
    let (_, _, _, cooldown) = client.stats();
    assert_eq!(cooldown, 120);
}

// ── event data validation tests ─────────────────────────

#[test]
fn test_withdraw_emits_event_with_correct_data() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&admin);
    client.deposit(&admin, &3_000_000);

    client.withdraw(&admin, &treasury, &1_000_000);

    let events = env.events().all();
    let withdraw_event = events.iter().find(|(_, topics, _)| {
        *topics
            == soroban_sdk::vec![
                &env,
                Symbol::new(&env, "withdraw").into_val(&env),
                admin.clone().into_val(&env),
            ]
    });
    assert!(withdraw_event.is_some(), "withdraw event not emitted");

    let (_, _, data) = withdraw_event.unwrap();
    let (to, amount): (Address, i128) = data.into_val(&env);
    assert_eq!(to, treasury);
    assert_eq!(amount, 1_000_000);
}

// ── transaction_exists tests ───────────────────────────

#[test]
fn test_transaction_exists_returns_true_for_valid_tx() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);
    let tx_id = client.send(&sender, &recipient, &200_000);

    assert!(client.transaction_exists(&tx_id));
}

#[test]
fn test_transaction_exists_returns_false_for_invalid_tx() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    client.init(&admin);

    assert!(!client.transaction_exists(&999));
    assert!(!client.transaction_exists(&0));
}

// ── security validation tests ──────────────────────────

#[test]
#[should_panic(expected = "admin cannot be the contract itself")]
fn test_init_with_contract_address_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceContract);
    let client = RemittanceContractClient::new(&env, &contract_id);
    // Try to use the contract's own address as admin
    client.init(&client.address);
}

#[test]
#[should_panic(expected = "treasury cannot be the contract itself")]
fn test_set_fee_with_contract_treasury_fails() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    client.init(&admin);

    client.set_fee(&500, &client.address);
}

#[test]
#[should_panic(expected = "metadata key must not be empty")]
fn test_set_user_metadata_empty_key_fails() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin);

    let key = soroban_sdk::String::from_str(&env, "");
    let value = soroban_sdk::String::from_str(&env, "verified");
    client.set_user_metadata(&user, &key, &value);
}

#[test]
#[should_panic(expected = "metadata value must not be empty")]
fn test_set_user_metadata_empty_value_fails() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin);

    let key = soroban_sdk::String::from_str(&env, "kyc_status");
    let value = soroban_sdk::String::from_str(&env, "");
    client.set_user_metadata(&user, &key, &value);
}

#[test]
fn test_set_user_metadata_long_key_succeeds() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    client.init(&admin);

    let key = soroban_sdk::String::from_str(&env, &"a".repeat(128));
    let value = soroban_sdk::String::from_str(&env, "verified");
    client.set_user_metadata(&user, &key, &value);

    let result = client.get_user_metadata(&user);
    assert!(result.is_some());
}

#[test]
fn test_transaction_exists_after_escrow() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let sender = Address::generate(&env);
    let recipient = Address::generate(&env);
    client.init(&admin);
    client.deposit(&sender, &1_000_000);
    let tx_id = client.escrow_funds(&sender, &recipient, &400_000, &0);

    assert!(client.transaction_exists(&tx_id));
}

#[test]
fn test_rate_limit_updated_emits_event_with_correct_data() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    client.init(&admin);

    client.set_rate_limit(&120);

    let events = env.events().all();
    let rate_event = events.iter().find(|(_, topics, _)| {
        *topics == soroban_sdk::vec![&env, Symbol::new(&env, "rate_limit_updated").into_val(&env),]
    });
    assert!(rate_event.is_some(), "rate_limit_updated event not emitted");

    let (_, _, data) = rate_event.unwrap();
    let cooldown: u64 = data.into_val(&env);
    assert_eq!(cooldown, 120);
}
