# ADR 0007: D04 — Provider-neutral AI execution, OCR, embeddings, and grounding policy

- **Status:** Proposed (dependency-independent P1 design slice for [#18](https://github.com/mESmaC/oia/issues/18); no runtime adapter or model evaluation is claimed by this ADR)
- **Date:** 2026-11-02 UTC
- **Decision ID:** D04 ([`docs/delivery.md`](../delivery.md#6-decision-register))
- **Requirements:** [`DOC-02`](../spec.md#41-public-document-library), [`DOC-03`](../spec.md#41-public-document-library), [`DOC-04`](../spec.md#41-public-document-library), [`DOC-05`](../spec.md#41-public-document-library), [`CASE-05`](../spec.md#31-investigation-workspace)
- **Architecture references:** [`docs/architecture.md` §2.3](../architecture.md#23-evidence-classes-and-graph), [`§5.1`](../architecture.md#51-proposed-initial-stores), [`§6.2`](../architecture.md#62-analysis-contracts), [`§6.3`](../architecture.md#63-ai-provider-abstraction), [`§9`](../architecture.md#9-access-collaboration-and-local-operation)
- **Companion contract:** [`docs/contracts/document-search-and-cited-qa.md`](../contracts/document-search-and-cited-qa.md)

## 1. Context

Issue [#18](https://github.com/mESmaC/oia/issues/18) covers document search (combining exact identifiers, filtered text, and optional semantic retrieval under `CASE-05` and `DOC-04`), cited document summaries (`DOC-03`), and grounded question answering (`DOC-04`, `DOC-05`). Delivery decision **D04** requires defining:

1. How local and remote OCR, embedding, and synthesis providers are separated from core domain and search workflows so no single model service is mandatory.
2. What workspace data-handling and remote-egress defaults govern private case material, source use conditions, and credentials.
3. How generated drafts, citations, review status, and model/run metadata map onto the shared architecture's origin classes (`docs/architecture.md` §2.3) and review states (`§2.4`) without inventing parallel domain records.
4. What empirical evaluation gate must be satisfied before any specific OCR engine, embedding model, or summarization/QA model is selected and claimed as supported in P1.

## 2. Decision

### 2.1 Provider-neutral capability boundaries (`oia-models` / worker adapters)

OIA separates model-assisted document capabilities into three independent adapter contracts behind `oia-models`, executed as bounded, cancellable jobs (`docs/architecture.md` §6.1–§6.3):

1. **Document OCR (`OcrProvider`):** Accepts a pinned original artifact reference, page/region bounds, language hints, and resource limits; emits a versioned extraction derivative (`conventional_derivative`) with explicit per-page/region coverage (`readable`, `unreadable`, `partial`) and passage/region anchors without modifying original bytes (`DOC-02`).
2. **Passage Embedding (`EmbeddingProvider`):** Accepts version-pinned extracted passage slices and emits fixed-dimension vectors into a disposable, rebuildable accelerator index (`docs/architecture.md` §5.1). Embeddings are derived search accelerators, never authoritative evidence records.
3. **Grounded Synthesis (`GroundedSynthesisProvider`):** Accepts a bounded, permission-filtered bundle of version-pinned source passages plus a versioned prompt/schema identifier for cited document summarization (`DOC-03`) or grounded question answering (`DOC-04`).

No AI or embedding provider is required for core case operation. When any or all of these capabilities are `unconfigured`, `disabled`, or `unavailable`, OIA preserves local document import, original viewing, UTF-8 text extraction, exact-identifier and filtered lexical search, passage annotation, release comparison, and cited report editing/export.

### 2.2 Workspace data-handling defaults and remote-egress policy

1. **Local model execution default (scope clarified 2026-11-03, #110):** OCR, embedding, and synthesis default to local model execution (loopback or in-process adapters with no external model-provider egress). This protects selected workspace material sent for inference; it does not disable public-source discovery, APIs, feeds, scraping, or other configured acquisition services under SOURCE-01–SOURCE-04. Transmitting document passages, queries, or metadata to a remote provider requires an explicit workspace configuration opt-in and surfaces a visible remote-provider disclosure before execution (`docs/architecture.md` §6.3).
2. **Authorization and source-condition filtering:** Before assembling any embedding or synthesis input batch, the application service filters candidate passages against:
   - The caller's workspace and case authorization scope (`docs/architecture.md` §9).
   - Each source record's `automated_processing` use condition (records prohibiting automated processing or remote transmission are excluded and recorded in the run's coverage omissions).
   - Secret and private-field exclusion: credential references, local filesystem paths, and non-selected case metadata are stripped via an allowlisted projection before input assembly.
3. **Untrusted input isolation by capability absence (`DOC-05`):** Prompt delimiters alone do not guarantee containment against prompt injection. The authoritative control is capability absence: the parsing, embedding, and synthesis execution paths are given no tool-invocation interface, no network client for document-supplied URLs, no credential access, and no authority to mutate permissions, review status, or case publication state (`AGENTS.md` §5). Model output is parsed strictly as passive structured data.

### 2.3 Shared origin classes, citation validation, and provenance

Model-assisted outputs use the architecture's single vocabulary (`docs/architecture.md` §2.3–§2.4, §6.2–§6.3) rather than defining a parallel evidence model:

- **Origin classes:** Model-generated summaries and QA responses are `analytical_inference` records produced by an `AnalysisRun`. Propositions asserted inside a source document remain `extracted_claim` records attributed to that exact `SourceRecord` version; a summary or QA response must never promote an `extracted_claim` or `analytical_inference` into a `normalized_observation` or an established OIA fact (`DOC-03`).
- **Provenance and numeric bounds:** Each `AnalysisRun` pins exact input record/artifact/extraction versions, provider and model identifiers/versions, prompt-template version, finite decoding parameters (`temperature` validated as finite within $[0.0, 2.0]$, optional integer `seed` when supported, finite integer token and byte budgets; non-finite `NaN` or `±Infinity` values are rejected), generation timestamp, and explicit input coverage (including `unreadable`, `missing`, `reference_only`, or budget-truncated pages).
- **Separate validation and review axes:**
  - *Structural citation validation* is a deterministic check performed by the application service confirming that every cited `(record_id, record_version_id, artifact_id, page, anchor)` resolves within the run's pinned input versions, falls on valid UTF-8 character boundaries (or valid page/region bounds), and matches the quoted text byte-for-byte. Structural validation proves the anchor exists in the pinned version; it does not establish semantic entailment.
  - *Human review status* (`unreviewed`, `in_review`, `reviewed`, `challenged`, `superseded`) lives on the separate review axis (`docs/architecture.md` §2.4). All generated drafts begin as `unreviewed`; model output attempting to set its own review status to `reviewed` or its claim assessment to `supported` is rejected.
- **Insufficient material and correction invalidation:** When retrieved passages do not answer a question or fail structural anchor validation, the service returns an explicit `insufficient_material` result rather than fabricating or repairing citations (`DOC-04`). When a cited source version is superseded or corrected, the dependency walk marks dependent summaries and QA runs `needs_reassessment` while keeping prior runs and their original version-pinned citations inspectable (`DOC-05`).

### 2.4 Deferred runtime/model selection and evaluation gate

This ADR settles the architectural boundary and safety rules for D04; selecting specific OCR, embedding, and summarization/QA models remains gated on empirical evaluation before P1 model release (`docs/delivery.md` §3, §6):

- Candidate OCR, embedding, and synthesis options must be evaluated on a documented corpus (including the deterministic A03 edge cases in [`fixtures/documents/a03/`](../../fixtures/documents/a03/README.md) and held-out documents), recording exact model versions, hardware/resource usage, anchor accuracy, coverage-omission honesty, refusal on insufficient material, and known failure modes before any provider or model is claimed as supported.

## 3. Alternatives and consequences

| Alternative | Reason for this choice |
| --- | --- |
| Require a hosted LLM and vector service for document search/reading | Violates local-first operation (`docs/architecture.md` §1.1, §6.3) and breaks offline cases when connectivity or credentials are absent. Exact-identifier and lexical search remain mandatory zero-model baselines. |
| Rely on prompt instructions alone to prevent prompt injection | Prompt wording can be bypassed by adversarial inputs; enforcing capability absence (no tool execution, network access, or state-mutation authority on the synthesis path) bounds the impact deterministically (`DOC-05`). |
| Treat model confidence or vector similarity as calibrated support | Similarity scores and model self-confidence are uncalibrated (`AGENTS.md` §2); structural anchor validation and human review remain distinct. |
| Rewrite citations in place when a source is corrected | Mutating issued summaries hides provenance history (`docs/architecture.md` §2.3, §6.2). Flagging `needs_reassessment` preserves the original run alongside the reassessed run. |

Every generated summary or QA response incurs a deterministic structural verification pass against retained passage bytes before publication, and source revisions create reassessment work rather than silent updates.

## 4. Validation and handoff

This slice is a design and fixture specification, not a runtime implementation or empirical model evaluation. The companion contract ([`docs/contracts/document-search-and-cited-qa.md`](../contracts/document-search-and-cited-qa.md)) and deterministic fixture validator ([`scripts/fixtures/verify_a03_qa.py`](../../scripts/fixtures/verify_a03_qa.py) over [`fixtures/documents/a03/qa_summary_expectations.json`](../../fixtures/documents/a03/qa_summary_expectations.json)) check the contract invariants against the merged A03 synthetic corpus. Runtime Rust implementation of #18 remains dependent on [#14](https://github.com/mESmaC/oia/issues/14), [#16](https://github.com/mESmaC/oia/issues/16), and [#17](https://github.com/mESmaC/oia/issues/17).
