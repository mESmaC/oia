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
The `regional` module adds a bounded `NEWS-02` regional-account kernel for
[issue #20](https://github.com/mESmaC/oia/issues/20): typed epistemic
categories, attributed-only stated purposes, analyst-assigned event grouping
with separate article/version counts and independent-source counts derived
from corpus lineage (not from declared roots), ordered legal-process stages,
and explicit window coverage. It does not acquire news, translate, or cluster
automatically.
Dependency locking is managed by the root workspace `Cargo.lock`.

See:

- [Contract specification](../../docs/contracts/news-claim-checking.md)
- [Implementation record and verification](../../docs/implementation/news-claim-checking.md)
- [Regional-account implementation record](../../docs/implementation/news-regional-accounts.md)

## Local verification

```text
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 clippy --workspace --all-targets --locked --offline -- -D warnings
cargo +1.99.0 test --workspace --locked --offline
cargo +1.99.0 test -p oia-news-claims --test review_contract --locked --offline
cargo +1.99.0 run -p oia-news-claims --example check-a06-claims --locked --offline
cargo +1.99.0 run -p oia-news-claims --example build-a06-regional-account --locked --offline
```
