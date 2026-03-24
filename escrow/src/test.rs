use super::{LiquifactEscrow, LiquifactEscrowClient};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env};

// ---------------------------------------------------------------------------
// Cost measurement infrastructure
//
// Profiling methodology
// ---------------------
// Soroban's test environment enables *invocation metering* by default.  After
// every contract call `env.cost_estimate().resources()` returns the CPU
// instructions and memory bytes consumed by that single invocation.  These
// numbers are deterministic for a given input shape, so they serve as stable
// regression baselines.
//
// Limitations
// -----------
// * Costs are measured against the *native* (non-Wasm) contract build used in
//   tests.  Real on-chain costs include Wasm VM overhead and are higher.
// * Transaction-size fees and XDR round-trip costs are NOT included.
// * Use `stellar-cli simulate` against a real RPC node for production fee
//   estimates.
//
// How to use
// ----------
// Call `measure_cost` immediately after the contract invocation you want to
// profile.  The helper prints a structured summary and returns a
// `CostMeasurement` you can assert upper-bound thresholds against.
// ---------------------------------------------------------------------------

/// Snapshot of resource consumption for a single contract invocation.
#[derive(Debug, Clone)]
pub struct CostMeasurement {
    /// Label identifying the operation being measured.
    pub label: &'static str,
    /// CPU instructions consumed (Soroban metering units).
    pub instructions: i64,
    /// Memory bytes allocated during the invocation.
    pub mem_bytes: i64,
}

impl CostMeasurement {
    /// Capture the cost of the most-recently-completed invocation.
    pub fn capture(env: &Env, label: &'static str) -> Self {
        let resources = env.cost_estimate().resources();
        let m = CostMeasurement {
            label,
            instructions: resources.instructions,
            mem_bytes: resources.mem_bytes,
        };
        // Print a structured line so `cargo test -- --nocapture` shows baselines.
        println!(
            "[cost] {:<30} instructions={:>12}  mem_bytes={:>10}",
            m.label, m.instructions, m.mem_bytes
        );
        m
    }

    /// Assert that instructions stay below `max_instructions`.
    ///
    /// Use this to catch performance regressions: if a refactor causes
    /// instruction count to exceed the recorded baseline the test fails with a
    /// clear message.
    pub fn assert_instructions_below(&self, max_instructions: i64) {
        assert!(
            self.instructions <= max_instructions,
            "[cost regression] '{}': instructions {} exceeded limit {}",
            self.label,
            self.instructions,
            max_instructions
        );
    }

    /// Assert that memory allocation stays below `max_mem_bytes`.
    pub fn assert_mem_below(&self, max_mem_bytes: i64) {
        assert!(
            self.mem_bytes <= max_mem_bytes,
            "[cost regression] '{}': mem_bytes {} exceeded limit {}",
            self.label,
            self.mem_bytes,
            max_mem_bytes
        );
    }
}

#[test]
fn test_init_and_get_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let sme = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    let escrow = client.init(
        &symbol_short!("INV001"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    assert_eq!(escrow.invoice_id, symbol_short!("INV001"));
    assert_eq!(escrow.amount, 10_000_0000000i128);
    assert_eq!(escrow.funded_amount, 0);
    assert_eq!(escrow.status, 0);

    let got = client.get_escrow();
    assert_eq!(got.invoice_id, escrow.invoice_id);
}

#[test]
fn test_fund_and_settle() {
    let env = Env::default();
    env.mock_all_auths();

    let sme = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(LiquifactEscrow, ());
    let client = LiquifactEscrowClient::new(&env, &contract_id);

    client.init(
        &symbol_short!("INV002"),
        &sme,
        &10_000_0000000i128,
        &800i64,
        &1000u64,
    );

    let escrow1 = client.fund(&investor, &10_000_0000000i128);
    assert_eq!(escrow1.funded_amount, 10_000_0000000i128);
    assert_eq!(escrow1.status, 1);

    let escrow2 = client.settle();
    assert_eq!(escrow2.status, 2);
}
