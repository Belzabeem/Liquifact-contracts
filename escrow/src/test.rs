use super::{LiquifactEscrow, LiquifactEscrowClient, MAX_INVESTORS_PER_ESCROW};
use soroban_sdk::{symbol_short, testutils::Address as _, xdr::ToXdr, Address, Env};

const DEFAULT_AMOUNT: i128 = 10_000_0000000;
const INVESTOR_MAP_XDR_SIZE_LIMIT_BYTES: u32 = 8_192;
const ESCROW_XDR_SIZE_LIMIT_BYTES: u32 = 9_216;
const FINAL_INSERT_WRITE_BYTES_LIMIT: u32 = 10_240;

fn setup_client(
    env: &Env,
    invoice_id: soroban_sdk::Symbol,
    amount: i128,
) -> LiquifactEscrowClient<'_> {
    env.mock_all_auths();

    let sme = Address::generate(env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(env, &contract_id);
    client.init(&invoice_id, &sme, &amount, &800i64, &1000u64);
    client
}

#[test]
fn test_init_and_get_escrow() {
    let env = Env::default();
    let client = setup_client(&env, symbol_short!("INV001"), DEFAULT_AMOUNT);

    let escrow = client.get_escrow();
    assert_eq!(escrow.invoice_id, symbol_short!("INV001"));
    assert_eq!(escrow.amount, DEFAULT_AMOUNT);
    assert_eq!(escrow.funded_amount, 0);
    assert_eq!(escrow.status, 0);
    assert_eq!(escrow.investor_contributions.len(), 0);
    assert_eq!(client.get_investor_count(), 0);
    assert_eq!(client.max_investors(), MAX_INVESTORS_PER_ESCROW);
}

#[test]
fn test_fund_tracks_investor_balances_and_settles() {
    let env = Env::default();
    let client = setup_client(&env, symbol_short!("INV002"), DEFAULT_AMOUNT);
    let investor = Address::generate(&env);

    let escrow1 = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(escrow1.funded_amount, 5_000_0000000i128);
    assert_eq!(escrow1.status, 0);
    assert_eq!(escrow1.investor_contributions.len(), 1);
    assert_eq!(client.get_investor_count(), 1);
    assert_eq!(
        client.get_investor_contribution(&investor),
        5_000_0000000i128
    );

    let escrow2 = client.fund(&investor, &5_000_0000000i128);
    assert_eq!(escrow2.funded_amount, DEFAULT_AMOUNT);
    assert_eq!(escrow2.status, 1);
    assert_eq!(escrow2.investor_contributions.len(), 1);
    assert_eq!(client.get_investor_contribution(&investor), DEFAULT_AMOUNT);

    let escrow3 = client.settle();
    assert_eq!(escrow3.status, 2);
}

#[test]
#[should_panic(expected = "Funding amount must be positive")]
fn test_rejects_zero_amount_funding() {
    let env = Env::default();
    let client = setup_client(&env, symbol_short!("INV003"), DEFAULT_AMOUNT);
    let investor = Address::generate(&env);

    client.fund(&investor, &0i128);
}

#[test]
fn test_existing_investor_can_top_up_after_cardinality_cap() {
    let env = Env::default();
    let client = setup_client(
        &env,
        symbol_short!("INV004"),
        i128::from(MAX_INVESTORS_PER_ESCROW) + 5,
    );

    let first_investor = Address::generate(&env);
    client.fund(&first_investor, &1i128);

    for _ in 1..MAX_INVESTORS_PER_ESCROW {
        let investor = Address::generate(&env);
        client.fund(&investor, &1i128);
    }

    assert_eq!(client.get_investor_count(), MAX_INVESTORS_PER_ESCROW);

    let escrow = client.fund(&first_investor, &5i128);
    assert_eq!(
        escrow.investor_contributions.len(),
        MAX_INVESTORS_PER_ESCROW
    );
    assert_eq!(client.get_investor_contribution(&first_investor), 6i128);
}

#[test]
#[should_panic(expected = "Investor limit exceeded")]
fn test_rejects_new_investor_beyond_supported_cardinality() {
    let env = Env::default();
    let client = setup_client(
        &env,
        symbol_short!("INV005"),
        i128::from(MAX_INVESTORS_PER_ESCROW) + 1,
    );

    for _ in 0..MAX_INVESTORS_PER_ESCROW {
        let investor = Address::generate(&env);
        client.fund(&investor, &1i128);
    }

    let overflow_investor = Address::generate(&env);
    client.fund(&overflow_investor, &1i128);
}

#[test]
fn test_storage_growth_regression_at_investor_cap() {
    let env = Env::default();
    let client = setup_client(
        &env,
        symbol_short!("INV006"),
        i128::from(MAX_INVESTORS_PER_ESCROW),
    );

    for _ in 0..MAX_INVESTORS_PER_ESCROW {
        let investor = Address::generate(&env);
        client.fund(&investor, &1i128);
    }

    let resources = env.cost_estimate().resources();
    let escrow = client.get_escrow();
    let investor_map_xdr_len = escrow.investor_contributions.clone().to_xdr(&env).len();
    let escrow_xdr_len = escrow.clone().to_xdr(&env).len();

    assert_eq!(
        escrow.investor_contributions.len(),
        MAX_INVESTORS_PER_ESCROW
    );
    assert!(
        investor_map_xdr_len <= INVESTOR_MAP_XDR_SIZE_LIMIT_BYTES,
        "investor map XDR footprint regressed: {} > {} bytes",
        investor_map_xdr_len,
        INVESTOR_MAP_XDR_SIZE_LIMIT_BYTES
    );
    assert!(
        escrow_xdr_len <= ESCROW_XDR_SIZE_LIMIT_BYTES,
        "escrow entry XDR footprint regressed: {} > {} bytes",
        escrow_xdr_len,
        ESCROW_XDR_SIZE_LIMIT_BYTES
    );
    assert_eq!(resources.write_entries, 1);
    assert!(
        resources.write_bytes <= FINAL_INSERT_WRITE_BYTES_LIMIT,
        "final investor insert write footprint regressed: {} > {} bytes",
        resources.write_bytes,
        FINAL_INSERT_WRITE_BYTES_LIMIT
    );
}
