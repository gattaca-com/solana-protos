# solana-protos

gRPC protos and the `sol-protos` Rust crate for the
[Titan Solana bundle API](https://docs.titanbuilder.xyz/solana). Titan is a
block builder on [GBX](https://docs.gravityblocks.xyz/), an open marketplace
for Solana blockspace; searchers send it bundles and transactions to include in
the blocks it builds.

The API is wire-compatible with the subset of the
[Jito searcher API](https://github.com/jito-labs/mev-protos) that Titan
supports, so existing Jito clients work after changing the URL. Unsupported
methods are omitted.

**Docs: <https://docs.titanbuilder.xyz/solana>**. They cover endpoints,
tipping, tip accounts and rate limits.

## Methods

| Method                        | Description                                              |
| ----------------------------- | -------------------------------------------------------- |
| `SendBundle`                  | Send a bundle of 1 to 5 signed transactions.             |
| `SendTransaction`             | Send a single signed transaction.                        |
| `GetConnectedLeaders`         | Connected validators and their upcoming leader slots.    |
| `GetConnectedLeadersRegioned` | The same, grouped by region.                             |
| `GetTipAccounts`              | The Titan tip accounts.                                  |
| `GetRegions`                  | Your current region and all available regions.           |

There is no bundle result stream; check landing on chain.

## Bundles

A bundle lands all or nothing, in order. Optional fields on top of Jito's
`Bundle`:

- `dropping_indices`: transactions that may be dropped if they fail.
- `slot`: only land in this slot.
- `target_accounts`, `target_bundle_id`: see `protos/bundle.proto`.

## Using the crate

```toml
[dependencies]
sol-protos = { git = "https://github.com/gattaca-com/solana-protos" }
tonic = { version = "0.14", features = ["tls-ring", "tls-webpki-roots"] }
```

[`examples/send_bundle.rs`](examples/send_bundle.rs) prints the tip accounts
and next connected leaders, then sends the given signed transactions (raw or
base64 files) as one bundle:

```sh
cargo run --example send_bundle -- https://sol-ams.titanbuilder.xyz tx1 tx2
```

Reuse one connection rather than reconnecting per bundle.

Other languages: compile `protos/` with any protobuf toolchain, or use a Jito
client. With `grpcurl`:

```sh
grpcurl -import-path protos -proto searcher.proto \
  sol-ams.titanbuilder.xyz:443 searcher.SearcherService/GetTipAccounts
```

## Development

Generated bindings are checked in under `src/generated`. After editing
`protos/*.proto`, regenerate them with:

```sh
REGENERATE_PROTO=1 cargo build
```
