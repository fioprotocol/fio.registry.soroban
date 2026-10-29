# Deployments

## Testnet

| | |
|---|---|
| Contract ID | `CDGZQS3P2TMYDDFEMLGGBWWG7SWAYGC2GY2CJYKRAHPPROXCPULLR6XE` |
| StellarExpert (source-verified) | https://stellar.expert/explorer/testnet/contract/CDGZQS3P2TMYDDFEMLGGBWWG7SWAYGC2GY2CJYKRAHPPROXCPULLR6XE |
| Release | [`v0.1.0_fio-registry_cli28.1.0`](https://github.com/fioprotocol/fio.registry.soroban/releases/tag/v0.1.0_fio-registry_cli28.1.0) (tag `v0.1.0`) |
| Wasm SHA-256 | `4b4edae27d27c1733fb04a5f878c4f37caead896341a178ca884997101fdb037` |
| Admin | `GBH7RXVLT3Q7XLOBLA3UYCS5BK4NYNRFYIZ62DVCXD5D6XZKM3ITCLFH` |
| Deployed | 2026-10-29 |

Demo data: public domain `fio`, handle `demo@fio` mapped to `XLM/XLM` and `FIO/FIO`.

```sh
stellar contract invoke --id CDGZQS3P2TMYDDFEMLGGBWWG7SWAYGC2GY2CJYKRAHPPROXCPULLR6XE \
  --network testnet --source-account <any funded identity> --send no -- \
  resolve --handle demo@fio --chain_code XLM --token_code XLM
# "GBH7RXVLT3Q7XLOBLA3UYCS5BK4NYNRFYIZ62DVCXD5D6XZKM3ITCLFH"
```

Stellar testnet is reset periodically (next reset 2026-12-16). After a reset the same release wasm is redeployed and
this file is updated with the new contract ID.
