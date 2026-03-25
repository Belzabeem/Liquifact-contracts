//! # LiquiFact Escrow Contract
//!
//! Holds investor funds for an invoice until settlement.
//!
//! ### Settlement Sequence
//! 1. **Initialization**: Admin creates the escrow with `init`.
//! 2. **Funding**: Investors contribute funds via `fund` until `funding_target` is met (status 0 -> 1).
//! 3. **Settlement**: SME calls `settle` to record buyer repayment (status 1 -> 2).
//! 4. **Claim**: Investors call `claim` to receive principal + yield post-settlement.
//!
//! # Storage Schema Versioning
//!
//! The escrow state is stored under versioned keys.
//!
//! ## Version history
//!
//! | Version | Changes |
//! |---------|---------|
//! | 1       | Initial schema with contribution tracking and claim flow. |

use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype, symbol_short, Address, Env, Symbol,
};

/// Current storage schema version.
pub const SCHEMA_VERSION: u32 = 1;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Escrow,
    Version,
    Contributions(Address),
    Claimed(Address),
}

/// Full state of an invoice escrow persisted in contract storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceEscrow {
    pub invoice_id: Symbol,
    pub admin: Address,
    pub sme_address: Address,
    pub amount: i128,
    pub funding_target: i128,
    pub funded_amount: i128,
    pub settled_amount: i128,
    pub yield_bps: u32,
    pub maturity: u64,
    /// Escrow lifecycle status:
    /// - `0` — **open**: accepting investor funding
    /// - `1` — **funded**: target met; awaiting settlement
    /// - `2` — **settled**: buyer paid; investors can redeem principal + yield
    pub status: u32,
    pub version: u32,
}

// ──────────────────────────────────────────────────────────────────────────────
// Event types
// ──────────────────────────────────────────────────────────────────────────────

#[contractevent]
pub struct EscrowInitialized {
    #[topic]
    pub name: Symbol,
    pub escrow: InvoiceEscrow,
}

#[contractevent]
pub struct EscrowFunded {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
    pub investor: Address,
    pub amount: i128,
    pub funded_amount: i128,
    pub status: u32,
}

#[contractevent]
pub struct EscrowSettled {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
    pub settled_amount: i128,
    pub total_due: i128,
    pub status: u32,
}

#[contractevent]
pub struct EscrowClaimed {
    #[topic]
    pub name: Symbol,
    pub invoice_id: Symbol,
    pub investor: Address,
    pub amount: i128,
}

#[contractevent]
pub struct MaturityUpdatedEvent {
    pub invoice_id: Symbol,
    pub old_maturity: u64,
    pub new_maturity: u64,
}

// ──────────────────────────────────────────────────────────────────────────────
// Contract
// ──────────────────────────────────────────────────────────────────────────────

#[contract]
pub struct LiquifactEscrow;

#[contractimpl]
impl LiquifactEscrow {
    /// Initialize a new invoice escrow.
    pub fn init(
        env: Env,
        admin: Address,
        invoice_id: Symbol,
        sme_address: Address,
        amount: i128,
        yield_bps: u32,
        maturity: u64,
    ) -> InvoiceEscrow {
        admin.require_auth();
        assert!(
            !env.storage().instance().has(&DataKey::Escrow),
            "Escrow already initialized"
        );
        assert!(amount > 0, "Escrow amount must be positive");

        let escrow = InvoiceEscrow {
            invoice_id,
            admin,
            sme_address,
            amount,
            funding_target: amount,
            funded_amount: 0,
            settled_amount: 0,
            yield_bps,
            maturity,
            status: 0,
            version: SCHEMA_VERSION,
        };

        env.storage().instance().set(&DataKey::Escrow, &escrow);
        env.storage()
            .instance()
            .set(&DataKey::Version, &SCHEMA_VERSION);

        EscrowInitialized {
            name: symbol_short!("init"),
            escrow: escrow.clone(),
        }
        .publish(&env);

        escrow
    }

    /// Return the current escrow state.
    pub fn get_escrow(env: Env) -> InvoiceEscrow {
        env.storage()
            .instance()
            .get(&DataKey::Escrow)
            .unwrap_or_else(|| panic!("Escrow not initialized"))
    }

    /// Returns the stored schema version.
    pub fn get_version(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::Version).unwrap_or(0)
    }

    /// Record investor funding.
    pub fn fund(env: Env, investor: Address, amount: i128) -> InvoiceEscrow {
        investor.require_auth();

        let mut escrow = Self::get_escrow(env.clone());
        assert!(amount > 0, "Funding amount must be positive");
        assert!(escrow.status == 0, "Escrow not open for funding");

        // Update total funded
        escrow.funded_amount += amount;
        if escrow.funded_amount >= escrow.funding_target {
            escrow.status = 1; // funded
        }

        // Update individual contribution
        let key = DataKey::Contributions(investor.clone());
        let current_contribution: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&key, &(current_contribution + amount));

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        EscrowFunded {
            name: symbol_short!("fund"),
            invoice_id: escrow.invoice_id.clone(),
            investor,
            amount,
            funded_amount: escrow.funded_amount,
            status: escrow.status,
        }
        .publish(&env);

        escrow
    }

    /// Get total contribution for an investor.
    pub fn get_contribution(env: Env, investor: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Contributions(investor))
            .unwrap_or(0)
    }

    /// Mark escrow as settled. In this version, we support partial settlement.
    pub fn settle(env: Env, amount: i128) -> InvoiceEscrow {
        let mut escrow = Self::get_escrow(env.clone());
        escrow.sme_address.require_auth();

        assert!(
            escrow.status == 1 || escrow.status == 2,
            "Escrow must be funded"
        );
        assert!(amount > 0, "Settlement amount must be positive");

        let interest = (escrow.amount * (escrow.yield_bps as i128)) / 10000;
        let total_due = escrow.amount + interest;

        escrow.settled_amount += amount;
        assert!(
            escrow.settled_amount <= total_due,
            "Settlement amount exceeds total due"
        );

        if escrow.settled_amount == total_due {
            escrow.status = 2; // fully settled
        }

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        EscrowSettled {
            name: symbol_short!("settle"),
            invoice_id: escrow.invoice_id.clone(),
            settled_amount: escrow.settled_amount,
            total_due,
            status: escrow.status,
        }
        .publish(&env);

        escrow
    }

    /// Claim principal and yield post-settlement.
    ///
    /// ### Invariants
    /// - **State**: Escrow must be in the `Settled` state (`status = 2`), meaning the buyer has fully repaid the invoice debt.
    /// - **Contribution**: The calling investor must have a recorded contribution from the funding phase.
    /// - **Idempotency**: Claims are one-time only. This function panics if the investor has already successfully claimed.
    ///
    /// ### Authorization
    /// Requires `investor.require_auth()`. This prevents any third party from claiming funds on behalf of the investor.
    ///
    /// ### Payout Calculation
    /// Payout = `principal + (principal * yield_bps / 10,000)`.
    pub fn claim(env: Env, investor: Address) -> i128 {
        investor.require_auth();

        let escrow = Self::get_escrow(env.clone());
        assert!(escrow.status == 2, "Escrow must be fully settled to claim");

        let contribution_key = DataKey::Contributions(investor.clone());
        let principal = env
            .storage()
            .persistent()
            .get(&contribution_key)
            .unwrap_or(0);
        assert!(principal > 0, "No contribution found to claim");

        // Check if already claimed
        let claimed_key = DataKey::Claimed(investor.clone());
        assert!(
            !env.storage().persistent().has(&claimed_key),
            "Already claimed"
        );

        // Payout = principal + (principal * yield_bps / 10000)
        let interest = (principal * (escrow.yield_bps as i128)) / 10000;
        let payout = principal + interest;

        // Mark as claimed
        env.storage().persistent().set(&claimed_key, &true);

        EscrowClaimed {
            name: symbol_short!("claim"),
            invoice_id: escrow.invoice_id.clone(),
            investor,
            amount: payout,
        }
        .publish(&env);

        payout
    }

    /// Update maturity timestamp. Only allowed by admin in Open state.
    pub fn update_maturity(env: Env, new_maturity: u64) -> InvoiceEscrow {
        let mut escrow = Self::get_escrow(env.clone());
        escrow.admin.require_auth();

        assert!(
            escrow.status == 0,
            "Maturity can only be updated in Open state"
        );

        let old_maturity = escrow.maturity;
        escrow.maturity = new_maturity;

        env.storage().instance().set(&DataKey::Escrow, &escrow);

        MaturityUpdatedEvent {
            invoice_id: escrow.invoice_id.clone(),
            old_maturity,
            new_maturity,
        }
        .publish(&env);

        escrow
    }

    /// Migrate storage (placeholder for future versions).
    pub fn migrate(env: Env, from_version: u32) -> u32 {
        let stored: u32 = env.storage().instance().get(&DataKey::Version).unwrap_or(0);
        assert!(stored == from_version, "from_version mismatch");
        assert!(from_version < SCHEMA_VERSION, "Already up to date");

        // No migrations yet
        panic!("No migration path");
    }
}

#[cfg(test)]
mod test;
