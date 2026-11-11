# solana-protos

Protocol definitions for the Titan Solana bundle API: the gRPC service searchers
use to send bundles and transactions into the blocks Titan builds, and the
`sol-protos` Rust crate with the generated client and server bindings.

The service is wire-compatible with the subset of the
[Jito block engine searcher API](https://github.com/jito-labs/mev-protos) that
Titan supports. A client written against Jito's `auth.proto` and
`searcher.proto` works against Titan after a change of URL and keypair.
Unsupported methods are omitted rather than stubbed.

The searcher documentation is at <https://docs.titanbuilder.xyz/api/solana>.
This README covers the protocol, authentication for registered searchers, the
validation errors, and the crate.

## Endpoints

| Region    | Endpoint                           |
| --------- | ---------------------------------- |
| Amsterdam | `https://sol-ams.titanbuilder.xyz` |
| Frankfurt | `https://sol-fra.titanbuilder.xyz` |
| London    | `https://sol-lon.titanbuilder.xyz` |
| Tokyo     | `https://sol-tyo.titanbuilder.xyz` |

gRPC over TLS on port 443. Every region feeds Titan builders in all regions; use
the one closest to you.

## Access

Unregistered use needs no registration or token: connect to an endpoint and
send, within 5 requests a second per client IP address across all
`SearcherService` calls. Searchers who need a higher budget ask the
Titan team on [Discord](https://discord.gg/hnerZ7dhTk), register the public
key of a Solana ed25519 keypair used only for authentication, and send a
bearer token with every call; see [Authentication](#authentication).

## Services

### `auth.AuthService`

Jito's challenge-response, unchanged on the wire.

| Method                  | Titan behaviour                                                                                   |
| ----------------------- | ------------------------------------------------------------------------------------------------- |
| `GenerateAuthChallenge` | Takes `role = SEARCHER` and the 32-byte public key; returns a 32-character challenge.             |
| `GenerateAuthTokens`    | Takes `challenge = "<pubkey base58>-<challenge>"`, the public key, and the 64-byte ed25519 signature of that string. Returns `access_token`; no `refresh_token`. |
| `RefreshAccessToken`    | `UNIMPLEMENTED`. Tokens do not expire; authenticate again on `UNAUTHENTICATED`.                   |

Registered searchers send `authorization: Bearer <access_token.value>` as
metadata on every `SearcherService` call; unregistered searchers send no
metadata.

### `searcher.SearcherService`

| Method                        | Titan behaviour                                                                                        |
| ----------------------------- | ------------------------------------------------------------------------------------------------------ |
| `SendBundle`                  | Validates the bundle synchronously and hands it to Titan builders in all regions. Returns the bundle id as 64 hex chars. |
| `SendTransaction`             | Titan addition. A single signed transaction into Titan blocks, competing on its priority fee.          |
| `GetConnectedLeaders`         | Every validator Titan builds for, with its upcoming leader slots (current and next epoch). All regions. |
| `GetConnectedLeadersRegioned` | The same grouped by region; no regions requested means all.                                            |
| `GetTipAccounts`              | The Titan tip accounts, sorted.                                                                        |
| `GetRegions`                  | The region you are connected to and every available region.                                            |

Not provided: `SubscribeBundleResults` and `GetNextScheduledLeader`. There is
no result stream; check landing on chain.

### `bundle.Bundle`

Jito's `Bundle` with four optional fields added. A Jito client that leaves
them unset sends a valid Titan bundle.

```protobuf
message Bundle {
  shared.Header header = 2;             // ignored
  repeated packet.Packet packets = 3;   // 1 to 5 signed transactions, in order
  repeated uint32 dropping_indices = 4; // transactions the builder may drop if they fail
  optional uint64 slot = 5;             // only land in this slot
  repeated bytes target_accounts = 6;   // up to 16 keys; reserved
  optional bytes target_bundle_id = 7;  // 32 bytes; land directly after that bundle
}
```

| Limit                        | Value                                |
| ---------------------------- | ------------------------------------ |
| Transactions per bundle      | 1 to 5                               |
| Transaction size             | 4,096 bytes                          |
| `SendBundle` request         | 24 KiB                               |
| Target accounts              | 16, distinct, 32 bytes each          |

A bundle lands whole and in order, apart from transactions at
`dropping_indices`, which are left out if they revert. Every signature is
verified at the API. The `jitodontfront` rule applies to static account keys.
The validation errors are listed [below](#validation-errors).

## Authentication

Only registered searchers authenticate; unregistered searchers skip this section.

1. `GenerateAuthChallenge` with `role = SEARCHER` (1) and the 32-byte public
   key. The reply is a 32-character challenge.
2. Sign the UTF-8 bytes of the string `"<pubkey base58>-<challenge>"` with the
   keypair's ed25519 key. The signature is 64 bytes.
3. `GenerateAuthTokens` with `challenge` set to that same string,
   `client_pubkey` to the 32-byte public key and `signed_challenge` to the
   signature. The reply carries `access_token.value`.
4. Send `authorization: Bearer <access_token.value>` as metadata on every
   `SearcherService` call.

The `authenticate` function in [`examples/send_bundle.rs`](examples/send_bundle.rs)
does this with the crate.

Token lifetime:

- The access token does not expire. `expires_at_utc` is set far in the future;
  a client that converts it to a date must treat an out-of-range value as
  "never".
- No refresh token is issued, and `RefreshAccessToken` returns
  `UNIMPLEMENTED: use generate_auth_tokens`.
- A token stops working when the API instance restarts or the key is
  deregistered. On `UNAUTHENTICATED`, run the flow again; there is no need to
  refresh on a timer.
- Each region issues its own tokens. Authenticate separately against each
  region you use.

A challenge is single-use, consumed by the `GenerateAuthTokens` call that
presents it. `AuthService` calls are limited to 100 a second per public key,
separately from the `SearcherService` budget.

| Method                  | Status               | Message                             | Cause |
| ----------------------- | -------------------- | ----------------------------------- | ----- |
| `GenerateAuthChallenge` | `INVALID_ARGUMENT`   | `unexpected role`                   | `role` is not `SEARCHER` (1). |
| both                    | `INVALID_ARGUMENT`   | `invalid pubkey`                    | The public key is not 32 bytes. |
| both                    | `INVALID_ARGUMENT`   | `unexpected pubkey`                 | The public key is not registered. |
| `GenerateAuthTokens`    | `INVALID_ARGUMENT`   | `invalid signature bytes`           | `signed_challenge` is not 64 bytes. |
| `GenerateAuthTokens`    | `PERMISSION_DENIED`  | `call generate_auth_challenge first` | No outstanding challenge for this key. |
| `GenerateAuthTokens`    | `INVALID_ARGUMENT`   | `unexpected challenge`              | `challenge` is not `"<pubkey>-<challenge>"` for an outstanding challenge of this key. |
| `GenerateAuthTokens`    | `PERMISSION_DENIED`  | `invalid signature`                 | The signature does not verify against the public key over the challenge string. |
| both                    | `RESOURCE_EXHAUSTED` | `rate limit exceeded`               | More than 100 auth calls in the current second for this key. |
| `RefreshAccessToken`    | `UNIMPLEMENTED`      | `use generate_auth_tokens`          | Always. |
| any `SearcherService`   | `UNAUTHENTICATED`    | `invalid auth header format`        | The value does not start with `Bearer `. |
| any `SearcherService`   | `UNAUTHENTICATED`    | `invalid bearer token`              | Unknown token, after a restart of the instance or deregistration of the key: authenticate again. |

## Validation errors

`SendBundle` and `SendTransaction` answer `INVALID_ARGUMENT` with one of these
messages:

| Message                                                                          | Cause |
| -------------------------------------------------------------------------------- | ----- |
| `packet too large`                                                               | The request is over 24 KiB, or a transaction is over 4,096 bytes. |
| `failed to decode Protobuf message`                                              | The request is not a valid `SendBundleRequest` / `SendTransactionRequest`. |
| `missing bundle`                                                                 | `bundle` is not set. |
| `missing transaction`                                                            | `transaction` is not set. |
| `bundle has no txs`                                                              | `packets` is empty. |
| `bundle has too many txs (max 5)`                                                | More than 5 packets. |
| `invalid transaction`                                                            | A transaction does not sanitize (malformed, wrong signer count, bad version). |
| `invalid signature`                                                              | A signature does not verify against its signer. |
| `bundle contains duplicate transaction`                                          | Two packets share a first signature. |
| `dropping index out of range`                                                    | An index in `dropping_indices` is not below the packet count. |
| `bundle has too many target accounts`                                            | More than 16 `target_accounts`. |
| `target account must be 32 bytes`                                                | A target account is not 32 bytes. |
| `bundle contains duplicate target account`                                       | A target account is listed twice. |
| `target bundle ID must be 32 bytes`                                              | `target_bundle_id` is set and not 32 bytes. |
| `a jitodontfront transaction must be at the front of the bundle`                 | A marked transaction follows an unmarked one. |
| `every jitodontfront transaction must share a signer with the first transaction` | A marked transaction in the leading run shares no signer with the first. |

Other statuses from the send calls: `RESOURCE_EXHAUSTED: rate limit exceeded`
(over the per-second budget), `DEADLINE_EXCEEDED: validation timed out` (the
server did not finish validating within 100 ms; resend) and
`UNAVAILABLE: shutting down` (the instance is restarting; resend). The read
calls answer `INTERNAL: failed to decode Protobuf message` for a request that
is not valid protobuf, and a method the API does not provide answers
`UNIMPLEMENTED`.

## Using the crate

```toml
[dependencies]
sol-protos = { git = "https://github.com/gattaca-com/solana-protos" }
tonic = { version = "0.14", features = ["tls-ring", "tls-webpki-roots"] }
```

`tls-webpki-roots` gives the client the public CA roots; `tls-ring` (or
`tls-aws-lc`) selects the rustls crypto provider. Without exactly one provider
feature in your dependency graph, rustls panics at the first TLS connection, so
either enable one here or install one at startup with
`rustls::crypto::ring::default_provider().install_default()`.

The crate exposes one module per `.proto` file (`auth`, `bundle`, `packet`,
`searcher`, `shared`) with `prost` messages and `tonic` clients
(`auth::auth_service_client::AuthServiceClient`,
`searcher::searcher_service_client::SearcherServiceClient`) and servers.
Bindings are generated with `tonic-prost-build` and checked in, so building
the crate needs no `protoc`.

[`examples/send_bundle.rs`](examples/send_bundle.rs) is a complete client:
it authenticates, prints the regions, tip accounts and connected leaders, and
sends a bundle of signed transactions read from files.

```sh
cargo run --example send_bundle -- \
  --url https://sol-ams.titanbuilder.xyz \
  --drop 0 trigger.tx backrun.tx
```

Add `--keypair <file>` to authenticate as a registered searcher.

Keep one HTTP/2 connection open and reuse it: a new TLS handshake costs more
than a `SendBundle` round trip. The example sets a keep-alive of 10 s
(`http2_keep_alive_interval`, `keep_alive_while_idle`) so the connection
survives idle periods between bundles.

### Other languages

The `.proto` files under [`protos/`](protos/) compile with any protobuf
toolchain (`buf generate`, `protoc`, `grpc-tools`). Jito's client libraries
also work, since the wire format is the same: point them at a Titan endpoint.
Unregistered use needs no keypair. With a registered keypair, three
differences to handle: `RefreshAccessToken` is unimplemented, so re-authenticate
instead of refreshing; the token reply has no `refresh_token`; and
`access_token.expires_at_utc` is set far in the future, so a client that
converts it to a date must accept an out-of-range value as "never".

With `grpcurl`, from this directory:

```sh
grpcurl -import-path protos -proto searcher.proto \
  sol-ams.titanbuilder.xyz:443 searcher.SearcherService/GetTipAccounts
```

As a registered searcher, add `-H "authorization: Bearer $TOKEN"`.

## Development

Edit `protos/*.proto`, then regenerate the bindings:

```sh
REGENERATE_PROTO=1 cargo build
```

CI runs `buf build` on the schemas, `buf breaking` against
`jito-labs/mev-protos` (`WIRE_JSON`) so the supported subset stays compatible,
and `cargo fmt --check` and `cargo check` on the crate. A field added to
`Bundle` must also be added to the bundle id derivation on the server.
