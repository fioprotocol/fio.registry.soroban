#![cfg(test)]
extern crate std;

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Env, String};

fn s(env: &Env, v: &str) -> String {
    String::from_str(env, v)
}

fn setup() -> (Env, FioRegistryClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let admin = Address::generate(&env);
    let id = env.register(FioRegistry, (&admin,));
    let client = FioRegistryClient::new(&env, &id);
    (env, client, admin)
}

#[test]
fn register_and_resolve() {
    let (env, c, _) = setup();
    let owner = Address::generate(&env);
    let exp = c.register_domain(&owner, &s(&env, "fio"), &true);
    assert_eq!(exp, 1_000_000 + TERM_SECS);

    c.register_handle(&owner, &s(&env, "Demo@FIO"));
    c.set_address(
        &s(&env, "demo@fio"),
        &s(&env, "xlm"),
        &s(&env, "xlm"),
        &s(&env, "GABC"),
    );
    c.set_address(
        &s(&env, "demo@fio"),
        &s(&env, "ETH"),
        &s(&env, "*"),
        &s(&env, "0xabc"),
    );

    assert_eq!(
        c.resolve(&s(&env, "demo@fio"), &s(&env, "XLM"), &s(&env, "XLM")),
        s(&env, "GABC")
    );
    // wildcard token fallback
    assert_eq!(
        c.resolve(&s(&env, "demo@fio"), &s(&env, "ETH"), &s(&env, "USDT")),
        s(&env, "0xabc")
    );

    let h = c.get_handle(&s(&env, "demo@fio"));
    assert_eq!(h.name, s(&env, "demo"));
    assert_eq!(h.domain, s(&env, "fio"));
    assert_eq!(h.owner, owner);
    assert_eq!(h.addresses.len(), 2);
}

#[test]
fn set_address_upserts_and_remove() {
    let (env, c, _) = setup();
    let owner = Address::generate(&env);
    c.register_domain(&owner, &s(&env, "fio"), &true);
    c.register_handle(&owner, &s(&env, "demo@fio"));
    let h = s(&env, "demo@fio");
    c.set_address(&h, &s(&env, "XLM"), &s(&env, "XLM"), &s(&env, "GOLD"));
    c.set_address(&h, &s(&env, "XLM"), &s(&env, "XLM"), &s(&env, "GNEW"));
    assert_eq!(c.get_handle(&h).addresses.len(), 1);
    assert_eq!(
        c.resolve(&h, &s(&env, "XLM"), &s(&env, "XLM")),
        s(&env, "GNEW")
    );

    c.remove_address(&h, &s(&env, "XLM"), &s(&env, "XLM"));
    assert_eq!(
        c.try_resolve(&h, &s(&env, "XLM"), &s(&env, "XLM")),
        Err(Ok(Error::AddressNotFound.into()))
    );
}

#[test]
fn private_domain_requires_domain_owner() {
    let (env, c, _) = setup();
    let dom_owner = Address::generate(&env);
    let other = Address::generate(&env);
    c.register_domain(&dom_owner, &s(&env, "acme"), &false);
    assert_eq!(
        c.try_register_handle(&other, &s(&env, "bob@acme")),
        Err(Ok(Error::DomainNotPublic.into()))
    );
    c.register_handle(&dom_owner, &s(&env, "alice@acme"));

    c.set_domain_public(&s(&env, "acme"), &true);
    c.register_handle(&other, &s(&env, "bob@acme"));
}

#[test]
fn duplicates_rejected_until_expired() {
    let (env, c, _) = setup();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    c.register_domain(&a, &s(&env, "fio"), &true);
    c.register_handle(&a, &s(&env, "x@fio"));
    assert_eq!(
        c.try_register_handle(&b, &s(&env, "x@fio")),
        Err(Ok(Error::HandleAlreadyRegistered.into()))
    );
    assert_eq!(
        c.try_register_domain(&b, &s(&env, "fio"), &true),
        Err(Ok(Error::DomainAlreadyRegistered.into()))
    );
    // keep the domain alive, let the handle lapse
    c.renew_domain(&s(&env, "fio"));
    env.ledger().set_timestamp(1_000_000 + TERM_SECS + 1);
    assert_eq!(
        c.try_resolve(&s(&env, "x@fio"), &s(&env, "XLM"), &s(&env, "XLM")),
        Err(Ok(Error::HandleExpired.into()))
    );
    c.register_handle(&b, &s(&env, "x@fio"));
    assert_eq!(c.get_handle(&s(&env, "x@fio")).owner, b);
}

#[test]
fn transfer_clears_addresses() {
    let (env, c, _) = setup();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    c.register_domain(&a, &s(&env, "fio"), &true);
    c.register_handle(&a, &s(&env, "x@fio"));
    c.set_address(
        &s(&env, "x@fio"),
        &s(&env, "XLM"),
        &s(&env, "XLM"),
        &s(&env, "GA"),
    );
    c.transfer_handle(&s(&env, "x@fio"), &b);
    let h = c.get_handle(&s(&env, "x@fio"));
    assert_eq!(h.owner, b);
    assert_eq!(h.addresses.len(), 0);

    c.transfer_domain(&s(&env, "fio"), &b);
    assert_eq!(c.get_domain(&s(&env, "fio")).owner, b);
}

#[test]
fn renew_extends_one_term() {
    let (env, c, _) = setup();
    let a = Address::generate(&env);
    c.register_domain(&a, &s(&env, "fio"), &true);
    let exp = c.register_handle(&a, &s(&env, "x@fio"));
    assert_eq!(c.renew_handle(&s(&env, "x@fio")), exp + TERM_SECS);
}

#[test]
fn migrate_preserves_expiration() {
    let (env, c, admin) = setup();
    let owner = Address::generate(&env);
    let exp = 1_000_000 + 3 * TERM_SECS;
    c.migrate_domain(&owner, &s(&env, "edge"), &false, &exp);
    c.migrate_handle(&owner, &s(&env, "satoshi@edge"), &exp);
    assert_eq!(c.get_handle(&s(&env, "satoshi@edge")).expiration, exp);
    assert_eq!(c.admin(), admin);
    assert_eq!(
        c.try_migrate_handle(&owner, &s(&env, "old@edge"), &999_999),
        Err(Ok(Error::InvalidExpiration.into()))
    );
}

#[test]
fn owner_auth_required() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let id = env.register(FioRegistry, (&admin,));
    let c = FioRegistryClient::new(&env, &id);
    let owner = Address::generate(&env);
    assert!(c
        .try_register_domain(&owner, &s(&env, "fio"), &true)
        .is_err());

    env.mock_all_auths();
    c.register_domain(&owner, &s(&env, "fio"), &true);
    c.register_handle(&owner, &s(&env, "x@fio"));
    assert_eq!(
        env.auths()[0].0,
        owner,
        "register_handle must be authorized by the owner"
    );
}

#[test]
fn validation_matches_fio_rules() {
    let (env, c, _) = setup();
    let a = Address::generate(&env);
    for bad in ["", "-fio", "fio-", "f_o", "a.b"] {
        assert_eq!(
            c.try_register_domain(&a, &s(&env, bad), &true),
            Err(Ok(Error::InvalidDomain.into())),
            "{bad}"
        );
    }
    assert_eq!(
        c.try_register_domain(&a, &s(&env, &"d".repeat(63)), &true),
        Err(Ok(Error::InvalidDomain.into()))
    );
    c.register_domain(&a, &s(&env, "fio"), &true);
    for bad in ["@fio", "x@", "nodomain", "x@@fio", "-x@fio", "x y@fio"] {
        assert_eq!(
            c.try_register_handle(&a, &s(&env, bad)),
            Err(Ok(Error::InvalidHandle.into())),
            "{bad}"
        );
    }
    let long = std::format!("{}@fio", "n".repeat(61));
    assert_eq!(
        c.try_register_handle(&a, &s(&env, &long)),
        Err(Ok(Error::InvalidHandle.into()))
    );

    c.register_handle(&a, &s(&env, "x@fio"));
    let h = s(&env, "x@fio");
    assert_eq!(
        c.try_set_address(&h, &s(&env, "TOOLONGCHAIN"), &s(&env, "X"), &s(&env, "G")),
        Err(Ok(Error::InvalidChainCode.into()))
    );
    assert_eq!(
        c.try_set_address(&h, &s(&env, "*"), &s(&env, "X"), &s(&env, "G")),
        Err(Ok(Error::InvalidChainCode.into()))
    );
    assert_eq!(
        c.try_set_address(&h, &s(&env, "XLM"), &s(&env, "X-Y"), &s(&env, "G")),
        Err(Ok(Error::InvalidTokenCode.into()))
    );
    assert_eq!(
        c.try_set_address(&h, &s(&env, "XLM"), &s(&env, "XLM"), &s(&env, "G A")),
        Err(Ok(Error::InvalidPublicAddress.into()))
    );
    assert_eq!(
        c.try_set_address(
            &h,
            &s(&env, "XLM"),
            &s(&env, "XLM"),
            &s(&env, &"a".repeat(129))
        ),
        Err(Ok(Error::InvalidPublicAddress.into()))
    );
}
