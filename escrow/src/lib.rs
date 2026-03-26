#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol, Vec,
};

pub const SCHEMA_VERSION: u32 = 1;
const LEGACY_ESCROW_KEY: Symbol = symbol_short!("escrow");

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceEscrow {
    pub invoice_id: Symbol,
    pub admin: Address,
    pub sme_address: Address,
    pub amount: i128,
    pub funding_target: i128,
    pub funded_amount: i128,
    pub yield_bps: i64,
    pub maturity: u64,
    /// 0 = open, 1 = funded, 2 = settled
    pub status: u32,
    pub version: u32,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Invoice(Symbol),
    InvoiceList,
    Version,
}

#[contract]
pub struct LiquifactEscrow;

#[contractimpl]
impl LiquifactEscrow {
    /// Initialize a new invoice escrow stored under `DataKey::Invoice(invoice_id)`.
    pub fn init(
        env: Env,
        admin: Address,
        invoice_id: Symbol,
        sme_address: Address,
        amount: i128,
        yield_bps: i64,
        maturity: u64,
    ) -> InvoiceEscrow {
        admin.require_auth();

        assert!(amount > 0, "Escrow amount must be positive");
        assert!(yield_bps <= 10_000, "yield_bps cannot exceed 10000");
        assert!(
            !Self::has_invoice(&env, &invoice_id),
            "Escrow already exists for this invoice"
        );

        let escrow = InvoiceEscrow {
            invoice_id: invoice_id.clone(),
            admin,
            sme_address,
            amount,
            funding_target: amount,
            funded_amount: 0,
            yield_bps,
            maturity,
            status: 0,
            version: SCHEMA_VERSION,
        };

        env.storage()
            .instance()
            .set(&DataKey::Invoice(invoice_id.clone()), &escrow);
        Self::append_invoice_id(&env, &invoice_id);

        if !env.storage().instance().has(&DataKey::Version) {
            env.storage().instance().set(&DataKey::Version, &SCHEMA_VERSION);
        }

        escrow
    }

    pub fn get_escrow(env: Env, invoice_id: Symbol) -> InvoiceEscrow {
        env.storage()
            .instance()
            .get(&DataKey::Invoice(invoice_id))
            .unwrap_or_else(|| panic!("Escrow not found for invoice"))
    }

    pub fn list_invoices(env: Env) -> Vec<Symbol> {
        Self::load_invoice_list(&env)
    }

    pub fn get_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Version)
            .unwrap_or(SCHEMA_VERSION)
    }

    pub fn fund(env: Env, invoice_id: Symbol, investor: Address, amount: i128) -> InvoiceEscrow {
        investor.require_auth();

        assert!(amount > 0, "Funding amount must be positive");

        let mut escrow = Self::get_escrow(env.clone(), invoice_id.clone());
        assert!(escrow.status == 0, "Escrow not open for funding");

        escrow.funded_amount = escrow
            .funded_amount
            .checked_add(amount)
            .unwrap_or_else(|| panic!("funded_amount overflow"));

        if escrow.funded_amount >= escrow.funding_target {
            escrow.status = 1;
        }

        env.storage()
            .instance()
            .set(&DataKey::Invoice(invoice_id), &escrow);

        escrow
    }

    pub fn settle(env: Env, invoice_id: Symbol) -> InvoiceEscrow {
        let mut escrow = Self::get_escrow(env.clone(), invoice_id.clone());

        escrow.sme_address.require_auth();
        assert!(escrow.status == 1, "Escrow must be funded before settlement");

        escrow.status = 2;
        env.storage()
            .instance()
            .set(&DataKey::Invoice(invoice_id), &escrow);

        escrow
    }

    pub fn update_maturity(env: Env, invoice_id: Symbol, new_maturity: u64) -> InvoiceEscrow {
        let mut escrow = Self::get_escrow(env.clone(), invoice_id.clone());

        escrow.admin.require_auth();
        assert!(escrow.status == 0, "Maturity can only be updated in Open state");

        escrow.maturity = new_maturity;
        env.storage()
            .instance()
            .set(&DataKey::Invoice(invoice_id), &escrow);

        escrow
    }

    pub fn transfer_admin(env: Env, invoice_id: Symbol, new_admin: Address) -> InvoiceEscrow {
        let mut escrow = Self::get_escrow(env.clone(), invoice_id.clone());

        escrow.admin.require_auth();
        assert!(
            escrow.admin != new_admin,
            "New admin must differ from current admin"
        );
        new_admin.require_auth();

        escrow.admin = new_admin;
        env.storage()
            .instance()
            .set(&DataKey::Invoice(invoice_id), &escrow);

        escrow
    }

    /// Migrates from the old singleton key (`"escrow"`) into keyed storage.
    /// Assumption: singleton payload already includes the intended `invoice_id`.
    pub fn migrate_singleton(env: Env, invoice_id: Symbol) -> InvoiceEscrow {
        assert!(
            !Self::has_invoice(&env, &invoice_id),
            "Escrow already exists for this invoice"
        );

        let escrow: InvoiceEscrow = env
            .storage()
            .instance()
            .get(&LEGACY_ESCROW_KEY)
            .unwrap_or_else(|| panic!("Legacy singleton escrow not found"));

        assert!(
            escrow.invoice_id == invoice_id,
            "Legacy escrow invoice_id mismatch"
        );

        env.storage()
            .instance()
            .set(&DataKey::Invoice(invoice_id.clone()), &escrow);
        env.storage().instance().remove(&LEGACY_ESCROW_KEY);
        Self::append_invoice_id(&env, &invoice_id);

        escrow
    }

    fn has_invoice(env: &Env, invoice_id: &Symbol) -> bool {
        env.storage()
            .instance()
            .has(&DataKey::Invoice(invoice_id.clone()))
    }

    fn load_invoice_list(env: &Env) -> Vec<Symbol> {
        env.storage()
            .instance()
            .get(&DataKey::InvoiceList)
            .unwrap_or(Vec::new(env))
    }

    fn append_invoice_id(env: &Env, invoice_id: &Symbol) {
        let mut list = Self::load_invoice_list(env);
        for existing in list.iter() {
            if existing == *invoice_id {
                return;
            }
        }

        list.push_back(invoice_id.clone());
        env.storage().instance().set(&DataKey::InvoiceList, &list);
    }
}

#[cfg(test)]
mod test;
