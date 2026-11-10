# solana-protos

Searcher protos backwards compatible with the [Jito](https://github.com/jito-labs/mev-protos) block engine interface.
Unsupported endpoints are intentionally omitted.

## Access

- Reach out to get your key whitelisted.
- The only supported role is `SEARCHER = 1`.

## Development

Generated bindings are checked in under `src/generated`. After editing
`protos/*.proto`, regenerate them with:

```sh
REGENERATE_PROTO=1 cargo build
```
