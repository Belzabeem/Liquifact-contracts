//! LiquiFact Escrow Contract
//!
//! Holds investor funds for an invoice until settlement.
//! - SME receives stablecoin when funding target is met
//! - Investors receive principal + yield when buyer pays at maturity

use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, Map, Symbol};

/// Product guardrail: a single escrow supports at most this many distinct
/// investors so the per-investor contribution map stays well below Soroban's
/// contract-data entry size limits.
pub const MAX_INVESTORS_PER_ESCROW: u32 = 128;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceEscrow {
    /// Unique invoice identifier (e.g. INV-1023)
    pub invoice_id: Symbol,
    /// SME wallet that receives liquidity
    pub sme_address: Address,
    /// Total amount in smallest unit (e.g. stroops for XLM)
    pub amount: i128,
    /// Funding target must be met to release to SME
    pub funding_target: i128,
    /// Total funded so far by investors
    pub funded_amount: i128,
    /// Yield basis points (e.g. 800 = 8%)
    pub yield_bps: i64,
    /// Maturity timestamp (ledger time)
    pub maturity: u64,
    /// Per-investor principal contributions for this invoice.
    ///
    /// This is intentionally bounded by `MAX_INVESTORS_PER_ESCROW` to prevent
    /// denial-of-storage patterns where attackers create too many distinct
    /// investor keys inside a single escrow instance.
    pub investor_contributions: Map<Address, i128>,
    /// Escrow status: 0 = open, 1 = funded, 2 = settled
    pub status: u32,
}

#[contract]
pub struct LiquifactEscrow;

#[contractimpl]
impl LiquifactEscrow {
    /// Initialize a new invoice escrow.
    pub fn init(
        env: Env,
        invoice_id: Symbol,
        sme_address: Address,
        amount: i128,
        yield_bps: i64,
        maturity: u64,
    ) -> InvoiceEscrow {
        let escrow = InvoiceEscrow {
            invoice_id: invoice_id.clone(),
            sme_address: sme_address.clone(),
            amount,
            funding_target: amount,
            funded_amount: 0,
            yield_bps,
            maturity,
            investor_contributions: Map::new(&env),
            status: 0, // open
        };
        env.storage()
            .instance()
            .set(&symbol_short!("escrow"), &escrow);
        escrow
    }

    /// Get current escrow state.
    pub fn get_escrow(env: Env) -> InvoiceEscrow {
        env.storage()
            .instance()
            .get(&symbol_short!("escrow"))
            .unwrap_or_else(|| panic!("Escrow not initialized"))
    }

    /// Product limit for distinct investors supported by one escrow.
    pub fn max_investors() -> u32 {
        MAX_INVESTORS_PER_ESCROW
    }

    /// Number of distinct investors recorded for this escrow.
    pub fn get_investor_count(env: Env) -> u32 {
        Self::get_escrow(env).investor_contributions.len()
    }

    /// Amount funded by a specific investor.
    pub fn get_investor_contribution(env: Env, investor: Address) -> i128 {
        Self::get_escrow(env)
            .investor_contributions
            .get(investor)
            .unwrap_or(0)
    }

    /// Record investor funding. In production, this would be called with token transfer.
    pub fn fund(env: Env, investor: Address, amount: i128) -> InvoiceEscrow {
        let mut escrow = Self::get_escrow(env.clone());
        assert!(escrow.status == 0, "Escrow not open for funding");
        assert!(amount > 0, "Funding amount must be positive");

        let previous_contribution = escrow
            .investor_contributions
            .get(investor.clone())
            .unwrap_or(0);
        if previous_contribution == 0 {
            assert!(
                escrow.investor_contributions.len() < MAX_INVESTORS_PER_ESCROW,
                "Investor limit exceeded"
            );
        }

        let updated_contribution = previous_contribution
            .checked_add(amount)
            .unwrap_or_else(|| panic!("Investor contribution overflow"));
        escrow
            .investor_contributions
            .set(investor, updated_contribution);
        escrow.funded_amount = escrow
            .funded_amount
            .checked_add(amount)
            .unwrap_or_else(|| panic!("Escrow funding overflow"));
        if escrow.funded_amount >= escrow.funding_target {
            escrow.status = 1; // funded - ready to release to SME
        }
        env.storage()
            .instance()
            .set(&symbol_short!("escrow"), &escrow);
        escrow
    }

    /// Mark escrow as settled (buyer paid). Releases principal + yield to investors.
    pub fn settle(env: Env) -> InvoiceEscrow {
        let mut escrow = Self::get_escrow(env.clone());
        assert!(
            escrow.status == 1,
            "Escrow must be funded before settlement"
        );
        escrow.status = 2; // settled
        env.storage()
            .instance()
            .set(&symbol_short!("escrow"), &escrow);
        escrow
    }
}

#[cfg(test)]
mod test;
