use super::{DataKey, InvoiceEscrow, LiquifactEscrow, LiquifactEscrowClient, SCHEMA_VERSION};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env};

fn deploy(env: &Env) -> (Address, LiquifactEscrowClient<'_>) {
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &contract_id);
    (contract_id, client)
}

fn init_invoice(
    client: &LiquifactEscrowClient,
    admin: &Address,
    invoice_id: &soroban_sdk::Symbol,
    sme: &Address,
    amount: &i128,
) {
    client.init(admin, invoice_id, sme, amount, &800i64, &1000u64);
}

#[test]
fn test_init_stores_keyed_invoice_and_lists_it() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let escrow = client.init(
        &admin,
        &symbol_short!("INV001"),
        &sme,
        &1_000i128,
        &800i64,
        &1000u64,
    );

    assert_eq!(escrow.invoice_id, symbol_short!("INV001"));
    assert_eq!(escrow.status, 0);
    assert_eq!(escrow.version, SCHEMA_VERSION);

    let list = client.list_invoices();
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).unwrap(), symbol_short!("INV001"));

    let stored = client.get_escrow(&symbol_short!("INV001"));
    assert_eq!(stored, escrow);
}

#[test]
#[should_panic(expected = "Escrow already exists for this invoice")]
fn test_init_duplicate_invoice_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    init_invoice(&client, &admin, &symbol_short!("INV002"), &sme, &1_000i128);
    init_invoice(&client, &admin, &symbol_short!("INV002"), &sme, &2_000i128);
}

#[test]
fn test_multiple_invoices_are_isolated() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);
    let sme_a = Address::generate(&env);
    let sme_b = Address::generate(&env);
    let investor = Address::generate(&env);

    init_invoice(&client, &admin, &symbol_short!("INV003"), &sme_a, &1_000i128);
    init_invoice(&client, &admin, &symbol_short!("INV004"), &sme_b, &2_000i128);

    client.fund(&symbol_short!("INV003"), &investor, &500i128);

    let e3 = client.get_escrow(&symbol_short!("INV003"));
    let e4 = client.get_escrow(&symbol_short!("INV004"));

    assert_eq!(e3.funded_amount, 500i128);
    assert_eq!(e3.status, 0);
    assert_eq!(e4.funded_amount, 0i128);
    assert_eq!(e4.status, 0);

    let list = client.list_invoices();
    assert_eq!(list.len(), 2);
}

#[test]
fn test_fund_then_settle_single_invoice() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);

    init_invoice(&client, &admin, &symbol_short!("INV005"), &sme, &1_000i128);

    let funded = client.fund(&symbol_short!("INV005"), &investor, &1_000i128);
    assert_eq!(funded.status, 1);

    let settled = client.settle(&symbol_short!("INV005"));
    assert_eq!(settled.status, 2);
}

#[test]
#[should_panic(expected = "Escrow not open for funding")]
fn test_fund_after_funded_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);

    init_invoice(&client, &admin, &symbol_short!("INV006"), &sme, &1_000i128);
    client.fund(&symbol_short!("INV006"), &investor, &1_000i128);
    client.fund(&symbol_short!("INV006"), &investor, &1i128);
}

#[test]
#[should_panic(expected = "Escrow must be funded before settlement")]
fn test_settle_unfunded_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    init_invoice(&client, &admin, &symbol_short!("INV007"), &sme, &1_000i128);
    client.settle(&symbol_short!("INV007"));
}

#[test]
#[should_panic(expected = "Escrow not found for invoice")]
fn test_get_unknown_invoice_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client) = deploy(&env);
    client.get_escrow(&symbol_short!("MISSING"));
}

#[test]
fn test_admin_transfer_and_maturity_update_are_scoped() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let sme = Address::generate(&env);

    init_invoice(&client, &admin, &symbol_short!("INV008"), &sme, &1_000i128);
    init_invoice(&client, &admin, &symbol_short!("INV009"), &sme, &2_000i128);

    let transferred = client.transfer_admin(&symbol_short!("INV008"), &admin2);
    assert_eq!(transferred.admin, admin2);

    let updated = client.update_maturity(&symbol_short!("INV008"), &2222u64);
    assert_eq!(updated.maturity, 2222u64);

    let untouched = client.get_escrow(&symbol_short!("INV009"));
    assert_eq!(untouched.admin, admin);
    assert_eq!(untouched.maturity, 1000u64);
}

#[test]
fn test_auths_recorded_for_key_actions() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client) = deploy(&env);
    let admin = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let sme = Address::generate(&env);
    let investor = Address::generate(&env);

    init_invoice(&client, &admin, &symbol_short!("INV010"), &sme, &1_000i128);
    client.fund(&symbol_short!("INV010"), &investor, &1_000i128);
    client.transfer_admin(&symbol_short!("INV010"), &admin2);
    client.settle(&symbol_short!("INV010"));

    let auths = env.auths();
    assert!(auths.iter().any(|(addr, _)| *addr == admin));
    assert!(auths.iter().any(|(addr, _)| *addr == investor));
    assert!(auths.iter().any(|(addr, _)| *addr == admin2));
    assert!(auths.iter().any(|(addr, _)| *addr == sme));
}

#[test]
#[should_panic(expected = "Legacy escrow invoice_id mismatch")]
fn test_migrate_singleton_invoice_mismatch_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client) = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let legacy = InvoiceEscrow {
        invoice_id: symbol_short!("LEGACY1"),
        admin,
        sme_address: sme,
        amount: 999i128,
        funding_target: 999i128,
        funded_amount: 0i128,
        yield_bps: 700i64,
        maturity: 777u64,
        status: 0,
        version: SCHEMA_VERSION,
    };

    env.as_contract(&contract_id, || {
        env.storage().instance().set(&symbol_short!("escrow"), &legacy);
    });

    client.migrate_singleton(&symbol_short!("WRONGID"));
}

#[test]
fn test_migrate_singleton_moves_to_keyed_storage() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client) = deploy(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    let legacy = InvoiceEscrow {
        invoice_id: symbol_short!("LEGACY2"),
        admin: admin.clone(),
        sme_address: sme.clone(),
        amount: 1_500i128,
        funding_target: 1_500i128,
        funded_amount: 100i128,
        yield_bps: 650i64,
        maturity: 9_999u64,
        status: 0,
        version: SCHEMA_VERSION,
    };

    env.as_contract(&contract_id, || {
        env.storage().instance().set(&symbol_short!("escrow"), &legacy);
    });

    let migrated = client.migrate_singleton(&symbol_short!("LEGACY2"));
    assert_eq!(migrated.invoice_id, symbol_short!("LEGACY2"));

    let stored = client.get_escrow(&symbol_short!("LEGACY2"));
    assert_eq!(stored, legacy);

    let list = client.list_invoices();
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).unwrap(), symbol_short!("LEGACY2"));

    env.as_contract(&contract_id, || {
        assert!(!env.storage().instance().has(&symbol_short!("escrow")));
        assert!(env
            .storage()
            .instance()
            .has(&DataKey::Invoice(symbol_short!("LEGACY2"))));
    });
}
