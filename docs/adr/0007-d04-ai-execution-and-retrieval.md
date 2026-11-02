# ADR 0007: AI Execution Boundary, Untrusted Input Isolation, and Retrieval Contracts (D04)

- **Date:** 2026-11-01
- **Status:** Proposed / Accepted for P1 research & retrieval design
- **Decision ID:** D04
- **Issues:** #18, #3 (parent epic), #14, #17
- **Requirements:** `DOC-03`, `DOC-04`, `DOC-05`, `CASE-05`
- **Acceptance Scenarios:** `A03` (cited document research), `A15` (historical analogue forecasting)

## Context

Requirement `D04` addresses the AI execution and retrieval boundary for Open Intelligence Agency (OIA). OIA is a local-first intelligence investigation suite. Document research requires:
1. Synthesizing complex multi-page public records, scientific papers, and intelligence reports into structured, inspectable draft summaries (`DOC-03`).
2. Providing filtered text and semantic search, grounded question answering with exact-version citations, and handling cases where available evidence is incomplete or insufficient (`DOC-04`).
3. Treating document text strictly as untrusted research input rather than system instructions (`DOC-05`), preventing prompt injection attacks from hijacking the application, executing tools, altering case permissions, or masquerading as verified facts.
4. Preserving model versions, configuration hashes, generation timestamps, human review status, and propagating dependency invalidation when source documents are updated or corrected (`DOC-05`).

Because OIA prioritizes local execution and verifiable provenance, the AI execution model must not require mandatory third-party cloud accounts, must strictly enforce data-handling boundaries, and must never present ungrounded model hallucinations as observations or facts.

## Decision

1. **Provider-Neutral Model Abstraction with Local-First Default:**
   - Define a shared provider trait (`ModelProvider` and `EmbeddingProvider`) in application service boundaries.
   - The default execution mode is **local-first** (`DataHandlingPolicy::LocalOnly`). Local inference is supported via embedded runtimes (e.g., Candle or ONNX Runtime for embeddings and lightweight models; local daemons such as llama.cpp / Ollama via localhost endpoints). Under `LocalOnly`, no document text, queries, or embeddings leave the local host.
   - Remote AI providers (OpenAI, Anthropic, Google, or custom OpenAI-compatible endpoints) require explicit per-workspace or per-case user authorization (`DataHandlingPolicy::AllowRemoteInference`). The UI must clearly disclose when data will be transmitted to external endpoints.

2. **Untrusted Input Isolation & Prompt Injection Containment (`DOC-05`):**
   - Source document text and user-provided research documents are classified as untrusted external data.
   - Document text injected into prompts is wrapped in rigid, randomized boundary delimiters (e.g., `<<<UNTRUSTED_DOCUMENT_CONTENT_<NONCE>>>> ... <<<END_UNTRUSTED_DOCUMENT_CONTENT_<NONCE>>>>`).
   - The system instructions explicitly enforce role separation: document text cannot alter system rules, declare itself verified, command tool execution, request credentials, or modify case state.
   - Model execution runs in an unprivileged capability context: the model engine cannot invoke shell commands, mutate files, or adjust user permissions. Tool execution is limited to read-only citation verification.

3. **Grounded Summarization Schema & Epistemic Separation (`DOC-03`):**
   - AI-generated summaries are generated as structured drafts (`DocumentSummary`), not unstructured prose.
   - The summary schema mandates:
     - Document identity, subject, and scope.
     - Extracted factual claims and findings, where **every substantive statement includes a `SourceCitation`**.
     - Document-declared methodology.
     - Document-stated limitations and risks.
     - Unresolved questions and data gaps.
   - Epistemic integrity: A document's raw assertion is classified as `OriginClass::SourceAssertion`. It must never be promoted to `OriginClass::NormalizedObservation` or established fact merely because a model summarized it. Model outputs themselves are strictly `OriginClass::AnalysisDerivative` (or `GeneratedUnchecked`).

4. **Grounded Question Answering & Honest Coverage (`DOC-04`):**
   - All question answering must synthesize answers exclusively from retrieved, cited passages.
   - If retrieved context is missing key facts, ambiguous, or contradictory, the engine must return a structured status of `InsufficientMaterial` or `AmbiguousContext` with documented query gaps, rather than hallucinating an answer.
   - Fabricating citations or citing non-existent anchors is strictly forbidden and caught by structural citation validators.

5. **Partial Processing & Truncation Transparency (`DOC-03`, `DOC-04`):**
   - When documents exceed model context budgets, contain unreadable OCR scans, or have corrupted pages, the output must include a `ProcessingCoverage` record.
   - `ProcessingCoverage` explicitly tracks `is_exhaustive: bool`, `total_bytes: usize`, `processed_bytes: usize`, `omitted_spans: Vec<OmittedSpan>`, and `truncation_reason: Option<TruncationReason>`.
   - Partial summaries must visibly alert the user to omitted sections.

6. **Provenance, Auditability & Invalidation Propagation (`DOC-05`):**
   - Every generated summary and QA answer preserves immutable provenance metadata: `model_id`, `provider_kind`, `temperature`, `prompt_template_id`, `prompt_template_hash`, `generation_timestamp_utc`, and `review_status: Unreviewed`.
   - When a source document version is superseded, corrected, or retracted, OIA's dependency graph flags all dependent summaries, claims, and QA records as `NeedsReassessment: true` rather than silently overwriting them.

7. **Hybrid Retrieval Architecture (`CASE-05`, `DOC-04`):**
   - Search combines exact lexical indexing (BM25 / trigram over extracted UTF-8 text with exact byte-offset tracking) with dense semantic vector embeddings.
   - Lexical search guarantees exact keyword, identifier, and phrase matching with zero hallucination.
   - Dense embeddings provide semantic similarity for conceptual exploration.
   - Search results preserve score, match type (`Lexical`, `Semantic`, `Hybrid`), matched byte spans, and filter boundaries.

## Alternatives Considered

- **Mandatory Cloud Model API (OpenAI/Anthropic only):** Rejected because it violates OIA's local-first architecture, breaks air-gapped/offline research, and leaks sensitive investigation documents without explicit user consent.
- **Unstructured Free-Text AI Summaries:** Rejected because free-text responses frequently drop source passage anchors, confuse document claims with objective truth, and fail `DOC-03` traceability requirements.
- **Allowing Document Text to Call Tools via Agentic Prompts:** Rejected because untrusted documents containing prompt injection could instruct the agent to exfiltrate private cases, delete data, or change user permissions (`DOC-05`).

## Requirement & Acceptance Traceability

| Requirement | Contract / Mechanism |
| ----------- | -------------------- |
| `DOC-03` | `DocumentSummary` schema with `SourceCitation` byte spans, explicit methodology/limitations/unresolved questions fields, and `ProcessingCoverage`. |
| `DOC-04` | Grounded QA engine returning `InsufficientMaterial` on missing data; hybrid BM25 + dense semantic search; exact version citation links. |
| `DOC-05` | Prompt isolation delimiters; `PromptTemplateMetadata` tracking; `NeedsReassessment` invalidation propagation; model output untrusted by default. |
| `CASE-05` | `SearchQuery` with scope filters, pagination, lexical/semantic hybrid weighting, and coverage limits. |
| `A03` | Evaluated against multi-page documents, unreadable OCR pages, adversarial prompt injections, and conflicting claims in `fixtures/ai/a03/`. |
| `A15` | Precedent and analogue retrieval preserving point-in-time cutoffs and source citations. |

## Validation

- Verified by offline deterministic test corpus and validator in `fixtures/ai/a03/` and `scripts/ai/verify_ai_contracts.py`.
- Tests prove:
  1. All citations point to exact, valid UTF-8 byte spans within source document bounds.
  2. Adversarial prompt injection payloads are contained and prevented from executing unauthorized state transitions.
  3. Insufficient material queries return typed `InsufficientMaterial` status with zero hallucinated citations.
  4. Truncated and partial documents generate valid `ProcessingCoverage` reporting `is_exhaustive: false` and disclosing omitted intervals.
