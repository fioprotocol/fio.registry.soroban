# FioRegistry (Soroban)

Minimal port of the [FIO Protocol](https://fio.net) registry to Stellar Soroban: FIO Domains, FIO Handles
(`name@domain`) and the per-handle mapping of `{chain_code, token_code} → public_address`. It mirrors the
`fionames` and `domains` tables of [`fio.address`](https://github.com/fioprotocol/fio.contracts/tree/master/contracts/fio.address)
and applies the same validation rules (`fio.common/fio_common_validator.hpp`).

This contract is the reference implementation for the FIO 2.0 migration of the FIO registry to Stellar.

## Contract

| Function | FIO equivalent | Auth |
|---|---|---|
| `__constructor(admin)` | | |
| `register_domain(owner, domain, is_public) -> expiration` | `regdomain` | owner |
| `set_domain_public(domain, is_public)` | `setdomainpub` | domain owner |
| `transfer_domain(domain, new_owner)` | `xferdomain` | domain owner |
| `renew_domain(domain) -> expiration` | `renewdomain` | domain owner |
| `register_handle(owner, handle) -> expiration` | `regaddress` | owner (+ domain public or owned by owner) |
| `set_address(handle, chain_code, token_code, public_address)` | `addaddress` | handle owner |
| `remove_address(handle, chain_code, token_code)` | `remaddress` | handle owner |
| `transfer_handle(handle, new_owner)` | `xferaddress` (clears mappings) | handle owner |
| `renew_handle(handle) -> expiration` | `renewaddress` | handle owner |
| `resolve(handle, chain_code, token_code) -> public_address` | `get_pub_address` (falls back to token `*`) | |
| `get_handle(handle)`, `get_domain(domain)` | `get_fio_names`, table reads | |
| `migrate_domain(owner, domain, is_public, expiration)`, `migrate_handle(owner, handle, expiration)` | snapshot import | admin |
| `admin()`, `set_admin(new_admin)`, `upgrade(wasm_hash)` | | admin |

Rules:
- Handle `name@domain`: 3–64 chars; name and domain are `[a-z0-9-]`, no leading/trailing `-`; domain 1–62 chars.
  Input is lowercased.
- Chain code: 1–10 chars of `[A-Za-z0-9$]`; token code the same or `*`. Stored uppercase.
- Public address: 1–128 chars, no spaces. Up to 200 mappings per handle.
- Registration and renewal terms are one year. An expired handle or domain does not resolve and can be registered again.
- Events: `domain_registered`, `handle_registered`, `address_set`, `address_removed`, `handle_transferred`,
  `domain_transferred`, `renewed`.
- Storage is persistent; every read or write extends the entry TTL to one year.

Fees, bundled transactions, FIO Requests and the FIO token are out of scope for this contract.

## Testnet deployment

See [DEPLOYMENTS.md](DEPLOYMENTS.md) for the contract ID and the source-verified StellarExpert link.

## Build and test

Requires Rust with the `wasm32v1-none` target and [Stellar CLI](https://developers.stellar.org/docs/tools/cli) 28.

```sh
cargo test
stellar contract build
```

Releases are built by [`stellar-expert/soroban-build-workflow`](https://github.com/stellar-expert/soroban-build-workflow)
on every `v*` tag. The workflow publishes the wasm as a GitHub release with a build-provenance attestation, which
StellarExpert uses to show the contract as source-verified. Deploy the wasm from the release, never a local build.

## Usage (testnet)

```sh
ID=<contract id from DEPLOYMENTS.md>
stellar keys generate alice --network testnet --fund

stellar contract invoke --id $ID --network testnet --source-account alice -- \
  register_domain --owner alice --domain mydomain --is_public true
stellar contract invoke --id $ID --network testnet --source-account alice -- \
  register_handle --owner alice --handle me@mydomain
stellar contract invoke --id $ID --network testnet --source-account alice -- \
  set_address --handle me@mydomain --chain_code XLM --token_code XLM --public_address "$(stellar keys address alice)"

stellar contract invoke --id $ID --network testnet --source-account alice --send no -- \
  resolve --handle demo@fio --chain_code XLM --token_code XLM
```

## FIO usage statistics

`scripts/fio-stats.sh` counts FIO mainnet registrations and FIO Requests over the last 12 months via the Hyperion
history API (requires `curl` and `jq`).

## License

MIT
