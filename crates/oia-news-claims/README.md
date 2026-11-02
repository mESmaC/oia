# oia-news-claims

Temporary standalone Rust validation package for the dependency-independent
claim-assessment, correction-propagation, and publisher-summary slice of
[issue #21](https://github.com/mESmaC/oia/issues/21) (`NEWS-03`, `NEWS-04`,
`NEWS-05`, and the claim-assessment subset of `A06`).

See:

- [Contract specification](../../docs/contracts/news-claim-checking.md)
- [Implementation record and verification](../../docs/implementation/news-claim-checking.md)

## Local verification

```text
cargo +1.99.0 fmt --manifest-path crates/oia-news-claims/Cargo.toml --all -- --check
cargo +1.99.0 clippy --manifest-path crates/oia-news-claims/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --manifest-path crates/oia-news-claims/Cargo.toml --workspace --locked --offline
cargo +1.99.0 run --manifest-path crates/oia-news-claims/Cargo.toml --example check-a06-claims --locked --offline
```
