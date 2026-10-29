//! FioRegistry: a minimal Soroban port of the FIO Protocol registry
//! (`fio.address` contract: `fionames` + `domains` tables).
//!
//! * Domains: `name`, owner, `is_public`, expiration.
//! * Handles (`name@domain`): owner, expiration and a list of
//!   `{chain_code, token_code, public_address}` mappings.
//! * `resolve` is the equivalent of FIO's `get_pub_address`.
//!
//! Validation rules are copied from `fio.common/fio_common_validator.hpp`.
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, panic_with_error, Address,
    BytesN, ContractExecutable, Env, String, Vec,
};

/// One year, the FIO registration and renewal term.
pub const TERM_SECS: u64 = 365 * 24 * 60 * 60;
pub const MAX_HANDLE_LEN: u32 = 64;
pub const MAX_DOMAIN_LEN: u32 = 62;
pub const MAX_CODE_LEN: u32 = 10;
pub const MAX_PUB_ADDRESS_LEN: u32 = 128;
/// Matches FIO's per-handle cap on mapped public addresses.
pub const MAX_ADDRESSES: u32 = 200;

const DAY_LEDGERS: u32 = 17_280;
const TTL_THRESHOLD: u32 = 30 * DAY_LEDGERS;
const TTL_EXTEND_TO: u32 = 365 * DAY_LEDGERS;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    InvalidHandle = 1,
    InvalidDomain = 2,
    InvalidChainCode = 3,
    InvalidTokenCode = 4,
    InvalidPublicAddress = 5,
    InvalidExpiration = 6,
    DomainNotFound = 7,
    DomainExpired = 8,
    DomainAlreadyRegistered = 9,
    DomainNotPublic = 10,
    HandleNotFound = 11,
    HandleExpired = 12,
    HandleAlreadyRegistered = 13,
    AddressNotFound = 14,
    TooManyAddresses = 15,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Domain {
    pub name: String,
    pub owner: Address,
    pub is_public: bool,
    pub expiration: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenPubAddress {
    pub chain_code: String,
    pub token_code: String,
    pub public_address: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Handle {
    pub name: String,
    pub domain: String,
    pub owner: Address,
    pub expiration: u64,
    pub addresses: Vec<TokenPubAddress>,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Domain(String),
    Handle(String),
}

#[contractevent(topics = ["domain_registered"])]
pub struct DomainRegistered {
    #[topic]
    pub domain: String,
    pub owner: Address,
    pub expiration: u64,
}

#[contractevent(topics = ["handle_registered"])]
pub struct HandleRegistered {
    #[topic]
    pub handle: String,
    pub owner: Address,
    pub expiration: u64,
}

#[contractevent(topics = ["address_set"])]
pub struct AddressSet {
    #[topic]
    pub handle: String,
    pub chain_code: String,
    pub token_code: String,
    pub public_address: String,
}

#[contractevent(topics = ["address_removed"])]
pub struct AddressRemoved {
    #[topic]
    pub handle: String,
    pub chain_code: String,
    pub token_code: String,
}

#[contractevent(topics = ["handle_transferred"])]
pub struct HandleTransferred {
    #[topic]
    pub handle: String,
    pub from: Address,
    pub to: Address,
}

#[contractevent(topics = ["domain_transferred"])]
pub struct DomainTransferred {
    #[topic]
    pub domain: String,
    pub from: Address,
    pub to: Address,
}

#[contractevent(topics = ["renewed"])]
pub struct Renewed {
    #[topic]
    pub name: String,
    pub expiration: u64,
}

#[contract]
pub struct FioRegistry;

#[contractimpl]
impl FioRegistry {
    /// `admin` is the migration authority: it imports the FIO snapshot
    /// (`migrate_*`) and can upgrade the contract.
    pub fn __constructor(env: Env, admin: Address) {
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    pub fn admin(env: Env) -> Address {
        read_admin(&env)
    }

    pub fn set_admin(env: Env, new_admin: Address) {
        read_admin(&env).require_auth();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        bump_instance(&env);
    }

    pub fn upgrade(env: Env, wasm_hash: BytesN<32>) {
        read_admin(&env).require_auth();
        env.deployer()
            .update_current_contract(ContractExecutable::Wasm(wasm_hash));
    }

    // ---- Domains ----

    /// Registers a domain for one term (FIO `regdomain`).
    pub fn register_domain(env: Env, owner: Address, domain: String, is_public: bool) -> u64 {
        owner.require_auth();
        let domain = canonical_domain(&env, &domain);
        let expiration = now(&env) + TERM_SECS;
        put_new_domain(&env, domain, owner, is_public, expiration);
        expiration
    }

    /// Imports a domain from the FIO chain snapshot with its original expiration.
    pub fn migrate_domain(
        env: Env,
        owner: Address,
        domain: String,
        is_public: bool,
        expiration: u64,
    ) {
        read_admin(&env).require_auth();
        let domain = canonical_domain(&env, &domain);
        if expiration <= now(&env) {
            panic_with_error!(&env, Error::InvalidExpiration);
        }
        put_new_domain(&env, domain, owner, is_public, expiration);
    }

    /// FIO `setdomainpub`.
    pub fn set_domain_public(env: Env, domain: String, is_public: bool) {
        let domain = canonical_domain(&env, &domain);
        let mut rec = live_domain(&env, &domain);
        rec.owner.require_auth();
        rec.is_public = is_public;
        save_domain(&env, &rec);
    }

    /// FIO `xferdomain`.
    pub fn transfer_domain(env: Env, domain: String, new_owner: Address) {
        let domain = canonical_domain(&env, &domain);
        let mut rec = live_domain(&env, &domain);
        rec.owner.require_auth();
        let from = rec.owner.clone();
        rec.owner = new_owner.clone();
        save_domain(&env, &rec);
        DomainTransferred {
            domain,
            from,
            to: new_owner,
        }
        .publish(&env);
    }

    /// FIO `renewdomain`: extends by one term.
    pub fn renew_domain(env: Env, domain: String) -> u64 {
        let domain = canonical_domain(&env, &domain);
        let mut rec = live_domain(&env, &domain);
        rec.owner.require_auth();
        rec.expiration += TERM_SECS;
        save_domain(&env, &rec);
        Renewed {
            name: domain,
            expiration: rec.expiration,
        }
        .publish(&env);
        rec.expiration
    }

    pub fn get_domain(env: Env, domain: String) -> Domain {
        let domain = canonical_domain(&env, &domain);
        load_domain(&env, &domain)
    }

    // ---- Handles ----

    /// Registers `name@domain` for one term (FIO `regaddress`). The domain must
    /// be live and either public or owned by `owner`.
    pub fn register_handle(env: Env, owner: Address, handle: String) -> u64 {
        owner.require_auth();
        let (handle, name, domain) = parse_handle(&env, &handle);
        let dom = live_domain(&env, &domain);
        if !dom.is_public && dom.owner != owner {
            panic_with_error!(&env, Error::DomainNotPublic);
        }
        let expiration = now(&env) + TERM_SECS;
        put_new_handle(&env, handle, name, domain, owner, expiration);
        expiration
    }

    /// Imports a handle from the FIO chain snapshot with its original expiration.
    pub fn migrate_handle(env: Env, owner: Address, handle: String, expiration: u64) {
        read_admin(&env).require_auth();
        let (handle, name, domain) = parse_handle(&env, &handle);
        load_domain(&env, &domain);
        if expiration <= now(&env) {
            panic_with_error!(&env, Error::InvalidExpiration);
        }
        put_new_handle(&env, handle, name, domain, owner, expiration);
    }

    /// FIO `addaddress` (single mapping, upsert).
    pub fn set_address(
        env: Env,
        handle: String,
        chain_code: String,
        token_code: String,
        public_address: String,
    ) {
        let (handle, _, _) = parse_handle(&env, &handle);
        let mut rec = live_handle(&env, &handle);
        rec.owner.require_auth();
        let chain_code = canonical_code(&env, &chain_code, Error::InvalidChainCode, false);
        let token_code = canonical_code(&env, &token_code, Error::InvalidTokenCode, true);
        validate_pub_address(&env, &public_address);

        let entry = TokenPubAddress {
            chain_code: chain_code.clone(),
            token_code: token_code.clone(),
            public_address: public_address.clone(),
        };
        match find_address(&rec.addresses, &chain_code, &token_code) {
            Some(i) => rec.addresses.set(i, entry),
            None => {
                if rec.addresses.len() >= MAX_ADDRESSES {
                    panic_with_error!(&env, Error::TooManyAddresses);
                }
                rec.addresses.push_back(entry);
            }
        }
        save_handle(&env, &rec);
        AddressSet {
            handle,
            chain_code,
            token_code,
            public_address,
        }
        .publish(&env);
    }

    /// FIO `remaddress` (single mapping).
    pub fn remove_address(env: Env, handle: String, chain_code: String, token_code: String) {
        let (handle, _, _) = parse_handle(&env, &handle);
        let mut rec = live_handle(&env, &handle);
        rec.owner.require_auth();
        let chain_code = canonical_code(&env, &chain_code, Error::InvalidChainCode, false);
        let token_code = canonical_code(&env, &token_code, Error::InvalidTokenCode, true);
        match find_address(&rec.addresses, &chain_code, &token_code) {
            Some(i) => {
                rec.addresses.remove(i);
            }
            None => panic_with_error!(&env, Error::AddressNotFound),
        }
        save_handle(&env, &rec);
        AddressRemoved {
            handle,
            chain_code,
            token_code,
        }
        .publish(&env);
    }

    /// FIO `xferaddress`. Mapped addresses are cleared, as on FIO.
    pub fn transfer_handle(env: Env, handle: String, new_owner: Address) {
        let (handle, _, _) = parse_handle(&env, &handle);
        let mut rec = live_handle(&env, &handle);
        rec.owner.require_auth();
        let from = rec.owner.clone();
        rec.owner = new_owner.clone();
        rec.addresses = Vec::new(&env);
        save_handle(&env, &rec);
        HandleTransferred {
            handle,
            from,
            to: new_owner,
        }
        .publish(&env);
    }

    /// FIO `renewaddress`: extends by one term.
    pub fn renew_handle(env: Env, handle: String) -> u64 {
        let (handle, _, _) = parse_handle(&env, &handle);
        let mut rec = live_handle(&env, &handle);
        rec.owner.require_auth();
        rec.expiration += TERM_SECS;
        save_handle(&env, &rec);
        Renewed {
            name: handle,
            expiration: rec.expiration,
        }
        .publish(&env);
        rec.expiration
    }

    pub fn get_handle(env: Env, handle: String) -> Handle {
        let (handle, _, _) = parse_handle(&env, &handle);
        load_handle(&env, &handle)
    }

    /// FIO `get_pub_address`: returns the public address mapped to
    /// `chain_code`/`token_code`, falling back to the chain-wide `*` token.
    pub fn resolve(env: Env, handle: String, chain_code: String, token_code: String) -> String {
        let (handle, _, domain) = parse_handle(&env, &handle);
        let rec = live_handle(&env, &handle);
        live_domain(&env, &domain);
        let chain_code = canonical_code(&env, &chain_code, Error::InvalidChainCode, false);
        let token_code = canonical_code(&env, &token_code, Error::InvalidTokenCode, true);
        let i = find_address(&rec.addresses, &chain_code, &token_code)
            .or_else(|| find_address(&rec.addresses, &chain_code, &String::from_str(&env, "*")))
            .unwrap_or_else(|| panic_with_error!(&env, Error::AddressNotFound));
        rec.addresses.get_unchecked(i).public_address
    }
}

// ---- storage ----

fn now(env: &Env) -> u64 {
    env.ledger().timestamp()
}

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
}

fn read_admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}

fn load_domain(env: &Env, domain: &String) -> Domain {
    let key = DataKey::Domain(domain.clone());
    let rec: Domain = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| panic_with_error!(env, Error::DomainNotFound));
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    rec
}

fn live_domain(env: &Env, domain: &String) -> Domain {
    let rec = load_domain(env, domain);
    if rec.expiration <= now(env) {
        panic_with_error!(env, Error::DomainExpired);
    }
    rec
}

fn save_domain(env: &Env, rec: &Domain) {
    let key = DataKey::Domain(rec.name.clone());
    env.storage().persistent().set(&key, rec);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    bump_instance(env);
}

fn put_new_domain(env: &Env, name: String, owner: Address, is_public: bool, expiration: u64) {
    let key = DataKey::Domain(name.clone());
    if let Some(existing) = env.storage().persistent().get::<_, Domain>(&key) {
        if existing.expiration > now(env) {
            panic_with_error!(env, Error::DomainAlreadyRegistered);
        }
    }
    save_domain(
        env,
        &Domain {
            name: name.clone(),
            owner: owner.clone(),
            is_public,
            expiration,
        },
    );
    DomainRegistered {
        domain: name,
        owner,
        expiration,
    }
    .publish(env);
}

fn load_handle(env: &Env, handle: &String) -> Handle {
    let key = DataKey::Handle(handle.clone());
    let rec: Handle = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| panic_with_error!(env, Error::HandleNotFound));
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    rec
}

fn live_handle(env: &Env, handle: &String) -> Handle {
    let rec = load_handle(env, handle);
    if rec.expiration <= now(env) {
        panic_with_error!(env, Error::HandleExpired);
    }
    rec
}

fn save_handle(env: &Env, rec: &Handle) {
    let mut full = [0u8; MAX_HANDLE_LEN as usize];
    let n = join_handle(&rec.name, &rec.domain, &mut full);
    let key = DataKey::Handle(String::from_bytes(env, &full[..n]));
    env.storage().persistent().set(&key, rec);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    bump_instance(env);
}

fn put_new_handle(
    env: &Env,
    handle: String,
    name: String,
    domain: String,
    owner: Address,
    expiration: u64,
) {
    let key = DataKey::Handle(handle.clone());
    if let Some(existing) = env.storage().persistent().get::<_, Handle>(&key) {
        if existing.expiration > now(env) {
            panic_with_error!(env, Error::HandleAlreadyRegistered);
        }
    }
    let rec = Handle {
        name,
        domain,
        owner: owner.clone(),
        expiration,
        addresses: Vec::new(env),
    };
    save_handle(env, &rec);
    HandleRegistered {
        handle,
        owner,
        expiration,
    }
    .publish(env);
}

fn find_address(list: &Vec<TokenPubAddress>, chain: &String, token: &String) -> Option<u32> {
    list.iter()
        .position(|a| a.chain_code == *chain && a.token_code == *token)
        .map(|i| i as u32)
}

// ---- validation (fio_common_validator.hpp) ----

fn join_handle(name: &String, domain: &String, out: &mut [u8]) -> usize {
    let (n, d) = (name.len() as usize, domain.len() as usize);
    name.copy_into_slice(&mut out[..n]);
    out[n] = b'@';
    domain.copy_into_slice(&mut out[n + 1..n + 1 + d]);
    n + 1 + d
}

/// `validateCharName`: `[a-z0-9-]+`, no leading/trailing hyphen.
fn valid_char_name(s: &[u8]) -> bool {
    !s.is_empty()
        && s[0] != b'-'
        && s[s.len() - 1] != b'-'
        && s.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

fn canonical_domain(env: &Env, domain: &String) -> String {
    let len = domain.len();
    if len < 1 || len > MAX_DOMAIN_LEN {
        panic_with_error!(env, Error::InvalidDomain);
    }
    let mut buf = [0u8; MAX_DOMAIN_LEN as usize];
    let s = &mut buf[..len as usize];
    domain.copy_into_slice(s);
    s.make_ascii_lowercase();
    if !valid_char_name(s) {
        panic_with_error!(env, Error::InvalidDomain);
    }
    String::from_bytes(env, s)
}

/// Splits and validates `name@domain`; returns canonical (lowercase)
/// `(handle, name, domain)`.
fn parse_handle(env: &Env, handle: &String) -> (String, String, String) {
    let len = handle.len();
    if len < 3 || len > MAX_HANDLE_LEN {
        panic_with_error!(env, Error::InvalidHandle);
    }
    let mut buf = [0u8; MAX_HANDLE_LEN as usize];
    let s = &mut buf[..len as usize];
    handle.copy_into_slice(s);
    s.make_ascii_lowercase();
    let at = match s.iter().position(|c| *c == b'@') {
        Some(i) => i,
        None => panic_with_error!(env, Error::InvalidHandle),
    };
    let (name, domain) = (&s[..at], &s[at + 1..]);
    if !valid_char_name(name) || !valid_char_name(domain) || domain.len() > MAX_DOMAIN_LEN as usize
    {
        panic_with_error!(env, Error::InvalidHandle);
    }
    (
        String::from_bytes(env, s),
        String::from_bytes(env, name),
        String::from_bytes(env, domain),
    )
}

/// `validateChainNameFormat` / `validateTokenNameFormat`: 1-10 chars of
/// `[A-Za-z0-9$]` (token may be `*`). Stored uppercase.
fn canonical_code(env: &Env, code: &String, err: Error, allow_wildcard: bool) -> String {
    let len = code.len();
    if len < 1 || len > MAX_CODE_LEN {
        panic_with_error!(env, err);
    }
    let mut buf = [0u8; MAX_CODE_LEN as usize];
    let s = &mut buf[..len as usize];
    code.copy_into_slice(s);
    let wildcard = allow_wildcard && s == b"*";
    if !wildcard && !s.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'$') {
        panic_with_error!(env, err);
    }
    s.make_ascii_uppercase();
    String::from_bytes(env, s)
}

/// `validatePubAddressFormat`: 1-128 chars, no spaces.
fn validate_pub_address(env: &Env, address: &String) {
    let len = address.len();
    if len < 1 || len > MAX_PUB_ADDRESS_LEN {
        panic_with_error!(env, Error::InvalidPublicAddress);
    }
    let mut buf = [0u8; MAX_PUB_ADDRESS_LEN as usize];
    let s = &mut buf[..len as usize];
    address.copy_into_slice(s);
    if s.contains(&b' ') {
        panic_with_error!(env, Error::InvalidPublicAddress);
    }
}

mod test;
