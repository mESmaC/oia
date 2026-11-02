# oia-news-claims

Workspace validation package for the dependency-independent
claim-assessment, quotation-fidelity, correction-propagation, and
publisher-summary consistency-checking kernel slice of
[issue #21](https://github.com/mESmaC/oia/issues/21) (`NEWS-03`, `NEWS-04`,
`NEWS-05`, and the claim-assessment subset of `A06`). Dependency locking is
managed by the root workspace `Cargo.lock`.

See:

- [Contract specification](../../docs/contracts/news-claim-checking.md)
- [Implementation record and verification](../../docs/implementation/news-claim-checking.md)

## Local verification

```text
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --workspace --locked --offline
cargo +1.99.0 run -p oia-news-claims --example check-a06-claims --locked --offline
```
