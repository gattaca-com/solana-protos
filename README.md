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

## Quick start

1. Call `GetConnectedLeaders` to find upcoming slots whose leader is connected
   to GBX. Bundles only land in those slots.
2. Pick the endpoint closest to the leader or your servers. No key or token is
   needed.
3. Send up to 5 signed transactions, including a tip to a Titan tip account,
   with `SendBundle`.

## Endpoints

| Region    | Endpoint                           |
| --------- | ---------------------------------- |
| Amsterdam | `https://sol-ams.titanbuilder.xyz` |
| Frankfurt | `https://sol-fra.titanbuilder.xyz` |
| London    | `https://sol-lon.titanbuilder.xyz` |
| Tokyo     | `https://sol-tyo.titanbuilder.xyz` |

Every region forwards to Titan builders in all regions.

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

## Tipping

- Minimum tip is 5000 lamports; larger tips get higher priority.
- Pick a random tip account per bundle so your bundles don't all write-lock the
  same account.
- Tip with a static instruction, not through CPI.

```
xk95T6h7nFSRZepEf6JGdv9BCvZLER6dDGXg2KmJ1t1
AEg8jYwFpEYXaVd3jMpxPu8V1HXvSvmZr9ms8wm4ub1a
HiLntniupDE3Y18j8BtoMtE6MxQdXRfJw3xYCKfBt3t5
8bXKHaeUzcYGr8ag8WJVdg2Etiy8vw3TCcb5f3dQ4xaq
HtmPJ2k1SzFzLE6Zdu7RXqGxgDPKx2C5Hkdu189B7TXe
5yjjgHRL8q4uuWXsg3CAvBJdLQ3NUDwJK17EjHHeCorX
DDceWhu2X1x38FWGzjPXnNGnKqJwPYHqTX4Xr2gd6xbW
2bERRYFDaZfMNN4TwiFLgm5wntZ457YnmpRBrpSUw9Aj
```

## Rate limits

5 requests per second by default. For higher limits or a custom setup, reach
out on [Discord](https://discord.gg/hnerZ7dhTk).

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
