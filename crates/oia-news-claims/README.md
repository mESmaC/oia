# oia-news-claims

Workspace validation package for the dependency-independent
claim-assessment, quotation-fidelity, correction-propagation, and
publisher-summary consistency-checking kernel, plus a bounded in-memory
append-only claim review ledger, for
[issue #21](https://github.com/mESmaC/oia/issues/21) (`NEWS-03`, `NEWS-04`,
`NEWS-05`, and the claim-assessment/review-history subsets of `A06`). The
ledger borrows a corpus-validated exact claim version and source passages; it
does not provide persistence, authentication/authorization, automated truth
evaluation, UI, application integration, or full end-to-end `A06`.
Dependency locking is managed by the root workspace `Cargo.lock`.

See:

- [Contract specification](../../docs/contracts/news-claim-checking.md)
- [Implementation record and verification](../../docs/implementation/news-claim-checking.md)

## Local verification

```text
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --workspace --locked --offline
cargo +1.99.0 test -p oia-news-claims --test review_contract --locked --offline
cargo +1.99.0 run -p oia-news-claims --example check-a06-claims --locked --offline
```
