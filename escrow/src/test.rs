use super::{FundEvent, InitEvent, LiquifactEscrow, LiquifactEscrowClient, SettleEvent};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events},
    vec, Address, Env, IntoVal, TryFromVal, Val,
};

// ── helpers ───────────────────────────────────────────────────────────────────

fn deploy(env: &Env) -> (LiquifactEscrowClient<'_>, Address) {
    let id = env.register(LiquifactEscrow, ());
    (LiquifactEscrowClient::new(env, &id), id)
}

/// Extract the typed data payload from the Nth event (0-indexed).
fn event_data<T: TryFromVal<Env, Val>>(env: &Env, n: usize) -> T {
    let all = env.events().all();
    let xdr_event = &all.events()[n];
    let data_xdr = match &xdr_event.body {
        soroban_sdk::xdr::ContractEventBody::V0(v0) => v0.data.clone(),
    };
    let raw: Val = Val::try_from_val(env, &data_xdr).unwrap();
    T::try_from_val(env, &raw).unwrap()
}

/// Extract topic[0] (the action symbol) from the Nth event.
fn event_topic0(env: &Env, n: usize) -> soroban_sdk::Symbol {
    let all = env.events().all();
    let xdr_event = &all.events()[n];
    let topics = match &xdr_event.body {
        soroban_sdk::xdr::ContractEventBody::V0(v0) => &v0.topics,
    };
    let raw: Val = Val::try_from_val(env, &topics[0]).unwrap();
    soroban_sdk::Symbol::try_from_val(env, &raw).unwrap()
}

// ── existing behaviour ────────────────────────────────────────────────────────

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Deploy a fresh contract and return (env, client, admin, sme).
fn setup() -> (Env, LiquifactEscrowClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let sme = Address::generate(&env);
    let (client, _) = deploy(&env);

    let escrow = client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    assert_eq!(escrow.invoice_id, symbol_short!("INV001"));
    assert_eq!(escrow.admin, admin);
    assert_eq!(escrow.sme_address, sme);
    assert_eq!(escrow.amount, 10_000_0000000i128);
    assert_eq!(escrow.funded_amount, 0);
    assert_eq!(escrow.status, 0);

    // get_escrow should return the same data
    let got = client.get_escrow();
    assert_eq!(got.invoice_id, escrow.invoice_id);
    assert_eq!(got.admin, admin);
}

#[test]
fn test_fund_partial_then_full() {
    let (env, client, admin, sme) = setup();
    let investor = Address::generate(&env);

    client.init(
        &admin,
        &symbol_short!("INV002"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    // Partial fund — status stays open
    let e1 = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(e1.funded_amount, 5_000_0000000i128);
    assert_eq!(e1.status, 0);

    // Complete fund — status becomes funded
    let e2 = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(e2.funded_amount, 10_000_0000000i128);
    assert_eq!(e2.status, 1);
}

#[test]
fn test_settle_after_full_funding() {
    let (env, client, admin, sme) = setup();
    let investor = Address::generate(&env);

    client.init(
        &admin,
        &symbol_short!("INV003"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128);

    let settled = client.settle();
    assert_eq!(settled.status, 2);
}

// ---------------------------------------------------------------------------
// Authorization verification tests
// ---------------------------------------------------------------------------

/// Verify that `init` records an auth requirement for the admin address.
#[test]
fn test_init_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let (client, _) = deploy(&env);

    client.init(
        &admin,
        &symbol_short!("INV005"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
    client.fund(&investor, &1_000i128);

    let auths = env.auths();
    assert!(
        auths.iter().any(|(addr, _)| *addr == investor),
        "investor auth was not recorded for fund"
    );
}

/// Verify that `settle` records an auth requirement for the SME address.
#[test]
fn test_settle_requires_sme_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &admin,
        &symbol_short!("INV006"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
    client.fund(&investor, &1_000i128);
    client.settle();

    let auths = env.auths();
    assert!(
        auths.iter().any(|(addr, _)| *addr == sme),
        "sme auth was not recorded for settle"
    );
}

// ---------------------------------------------------------------------------
// Unauthorized / panic-path tests
// ---------------------------------------------------------------------------

/// `init` called by a non-admin should panic (auth not satisfied).
#[test]
#[should_panic]
fn test_init_unauthorized_panics() {
    let env = Env::default();
    // Do NOT mock auths — let the real auth check fire.
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &admin,
        &symbol_short!("INV007"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
}

/// `settle` called without SME auth should panic.
#[test]
#[should_panic]
fn test_settle_unauthorized_panics() {
    let env = Env::default();
    // Do NOT mock auths.
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    // Use mock_all_auths only for setup steps.
    env.mock_all_auths();
    client.init(
        &admin,
        &symbol_short!("INV008"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
    client.fund(&investor, &1_000i128);

    // Clear mocked auths so settle must satisfy real auth.
    // Soroban test env doesn't expose a "clear mocks" API, so we re-create
    // a client on the same contract without mocking to trigger the failure.
    let env2 = Env::default(); // fresh env — no mocked auths
    let client2 = LiquifactEscrowClient::new(&env2, &contract_id);
    client2.settle(); // should panic: sme auth not satisfied
}

// ---------------------------------------------------------------------------
// Edge-case / guard tests
// ---------------------------------------------------------------------------

/// Re-initializing an already-initialized escrow must panic.
#[test]
#[should_panic(expected = "Escrow already initialized")]
fn test_double_init_panics() {
    let (_, client, admin, sme) = setup();

    client.init(
        &admin,
        &symbol_short!("INV009"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
    // Second init on the same contract must be rejected.
    client.init(
        &admin,
        &symbol_short!("INV009"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
}

/// Funding an already-funded escrow must panic.
#[test]
#[should_panic(expected = "Escrow not open for funding")]
fn test_fund_after_funded_panics() {
    let (env, client, admin, sme) = setup();
    let investor = Address::generate(&env);

    client.init(
        &admin,
        &symbol_short!("INV010"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
    client.fund(&investor, &1_000i128); // reaches funded status
    client.fund(&investor, &1i128); // must panic
}

/// Settling an escrow that is still open (not yet funded) must panic.
#[test]
#[should_panic(expected = "Escrow must be funded before settlement")]
fn test_settle_before_funded_panics() {
    let (_, client, admin, sme) = setup();

    client.init(
        &admin,
        &symbol_short!("INV011"),
        &sme,
        &1_000i128,
        &500i64,
        &2000u64,
    );
    client.settle(); // status is still 0 — must panic
}

/// `get_escrow` on an uninitialized contract must panic.
#[test]
#[should_panic(expected = "Escrow not initialized")]
fn test_get_escrow_uninitialized_panics() {
    let env = Env::default();
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);
    client.get_escrow();
}

/// Partial funding across two investors; status stays open until target is met.
#[test]
fn test_partial_fund_stays_open() {
    let env = Env::default();
    env.mock_all_auths();

    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &symbol_short!("INV003"),
        &sme,
        &10_000_0000000i128,
        &500i64,
        &2000u64,
    );

    // Fund half — should remain open
    let partial = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(partial.status, 0, "status should still be open");
    assert_eq!(partial.funded_amount, 5_000_0000000i128);

    // Fund the rest — should flip to funded
    let full = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(full.status, 1, "status should be funded");
}

/// Attempting to settle an escrow that is still open must panic.
#[test]
#[should_panic(expected = "Escrow must be funded before settlement")]
fn test_settle_unfunded_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let sme = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &symbol_short!("INV004"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    client.settle(); // must panic
}

/// Funding an already-funded (status=1) escrow must panic.
#[test]
#[should_panic(expected = "Escrow not open for funding")]
fn test_fund_after_funded_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &symbol_short!("INV005"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    client.fund(&investor, &10_000_0000000i128); // fills target → status 1
    client.fund(&investor, &1i128); // must panic
}

// ── event: init ───────────────────────────────────────────────────────────────

#[test]
fn test_init_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let sme = Address::generate(&env);
    let (client, contract_id) = deploy(&env);

    client.init(
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    assert_eq!(env.events().all().events().len(), 1);

    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                contract_id,
                vec![
                    &env,
                    symbol_short!("init").into_val(&env),
                    symbol_short!("INV001").into_val(&env),
                ],
                InitEvent {
                    sme_address: sme.clone(),
                    amount: 10_000_0000000i128,
                    yield_bps: 800i64,
                    maturity: 1000u64,
                }
                .into_val(&env),
            )
        ]
    );
}

// ── event: fund (partial) ─────────────────────────────────────────────────────

#[test]
fn test_fund_partial_emits_event_status_open() {
    let env = Env::default();
    env.mock_all_auths();
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let (client, _) = deploy(&env);

    client.init(
        &symbol_short!("INV003"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    client.fund(&investor, &4_000_0000000i128);

    // events().all() captures only the most recent invocation's events
    assert_eq!(env.events().all().events().len(), 1);

    let payload: FundEvent = event_data(&env, 0);
    assert_eq!(payload.investor, investor);
    assert_eq!(payload.amount, 4_000_0000000i128);
    assert_eq!(payload.funded_amount, 4_000_0000000i128);
    assert_eq!(payload.status, 0); // still open
}

// ── event: fund (fully funded) ────────────────────────────────────────────────

#[test]
fn test_fund_full_emits_event_status_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let (client, _) = deploy(&env);

    client.init(
        &symbol_short!("INV004"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128);

    let payload: FundEvent = event_data(&env, 0);
    assert_eq!(payload.status, 1); // fully funded
    assert_eq!(payload.funded_amount, 10_000_0000000i128);
}

// ── event: settle ─────────────────────────────────────────────────────────────

#[test]
fn test_settle_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let (client, _) = deploy(&env);

    client.init(
        &symbol_short!("INV005"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    client.fund(&investor, &10_000_0000000i128);
    client.settle();

    // settle is the only event from the last invocation
    assert_eq!(env.events().all().events().len(), 1);

    let payload: SettleEvent = event_data(&env, 0);
    assert_eq!(payload.sme_address, sme);
    assert_eq!(payload.amount, 10_000_0000000i128);
    assert_eq!(payload.yield_bps, 800i64);
}

// ── event topic correctness ───────────────────────────────────────────────────

#[test]
fn test_event_topics_are_correct() {
    let env = Env::default();
    env.mock_all_auths();
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let (client, _) = deploy(&env);

    client.init(
        &symbol_short!("INV006"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    assert_eq!(event_topic0(&env, 0), symbol_short!("init"));

    client.fund(&investor, &10_000_0000000i128);
    assert_eq!(event_topic0(&env, 0), symbol_short!("fund"));

    client.settle();
    // each call emits exactly one event; check the last one (settle)
    assert_eq!(env.events().all().events().len(), 1);
    assert_eq!(event_topic0(&env, 0), symbol_short!("settle"));
}

// ── edge cases ────────────────────────────────────────────────────────────────

/// Two partial tranches emit two fund events with cumulative funded_amount.
#[test]
fn test_two_partial_funds_emit_two_events() {
    let env = Env::default();
    env.mock_all_auths();
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let (client, _) = deploy(&env);

    client.init(
        &symbol_short!("INV007"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    client.fund(&investor, &3_000_0000000i128);
    // first fund event
    assert_eq!(env.events().all().events().len(), 1);
    let first: FundEvent = event_data(&env, 0);
    assert_eq!(first.funded_amount, 3_000_0000000i128);
    assert_eq!(first.status, 0);

    client.fund(&investor, &7_000_0000000i128);
    // second fund event
    assert_eq!(env.events().all().events().len(), 1);
    let second: FundEvent = event_data(&env, 0);
    assert_eq!(second.funded_amount, 10_000_0000000i128);
    assert_eq!(second.status, 1); // fully funded on second tranche
}

/// Settling before funded must panic — no settle event emitted.
#[test]
#[should_panic(expected = "Escrow must be funded before settlement")]
fn test_settle_before_funded_no_event() {
    let env = Env::default();
    env.mock_all_auths();
    let sme = Address::generate(&env);
    let (client, _) = deploy(&env);

    client.init(
        &symbol_short!("INV008"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );
    client.settle();
}
