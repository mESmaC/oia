# ADR 0006: report and investigation export formats (D05)

Date: 2026-11-02 UTC. Status: selected design for implementation; no exporter or
importer is implemented by this ADR. Scope: the dependency-independent design slice
of [#22](https://github.com/mESmaC/oia/issues/22).

## Context

Researchers need a readable report and a portable investigation with inspectable
evidence. Those are different outputs: a report can be read without OIA, while a
bundle must preserve version identities, permitted bytes, citations, omissions,
and the inputs needed to understand reproducibility limits. A report by itself
cannot reconstruct the investigation.

[COLLAB-02/COLLAB-03](../spec.md#71-cases-and-reports),
[REPRO-01/REPRO-02](../spec.md#72-reproducible-packages), and
[UX-05](../spec.md#32-visual-identity) require this
distinction. The [architecture](../architecture.md#10-exports-and-portability)
already proposes Markdown, self-contained HTML, and a JSON bundle, with PDF later.
No new renderer, storage engine, runtime dependency, or project license is selected
here. D01, D02, D04, and D14 retain their separate owners.

## Decision

Use a shared, version-pinned report projection to produce two readable formats:

- UTF-8 Markdown with visible evidence labels, numbered citations, coverage,
  omissions, and an evidence inventory. Plain-text reading remains useful even
  when a Markdown viewer does not support fragment links or tables.
- A single self-contained UTF-8 HTML file with inline styling, semantic headings,
  print styles, internal citation links, and no script, active embeds, remote
  styles/fonts/media, or automatic network requests. Permitted preview images may
  be embedded within explicit limits. Omitted media remain labeled references.

Use an uncompressed directory bundle for the first portable investigation format:
JSON manifest and versioned records, content-addressed permitted assets, optional
reports, and reproduction manifests. Directory import is read-only with respect
to its input. It stages and verifies content in managed storage before atomic case
creation. Import does not run reproduction instructions or activate providers.

The [report and bundle contract](../contracts/report-export.md) specifies the
behavior and handoff to the runtime implementer. The manifest is a transport
envelope over the shared domain serialization, not a second evidence model. Its
first wire schema must be finalized with the merged domain and storage contracts
before a compatibility version is published. This ADR does not declare a shipped
bundle version or commit to field spellings absent those contracts.

Saving, private export, collaborator sharing, and publication remain distinct
application commands. Export writes only the chosen local destination. Import
creates a private case owned by the importing workspace; source role names or
actor identifiers never grant local permissions.

PDF and archive containers are deferred. A ZIP file presented to this initial
importer yields an unsupported-container result; it is not automatically unpacked.
Supporting archives later requires a separate bounded extraction contract covering
expanded sizes, entry counts, compression ratios, links, and duplicate names.

## Alternatives and consequences

| Alternative | Reason for this choice |
| --- | --- |
| HTML alone | Portable reading is useful, but rendered prose loses structured edits and much of the investigation. JSON remains the round-trip format. |
| Markdown alone | Easy to inspect and edit, but viewers vary and cannot consistently provide print layout or internal evidence navigation. Offer HTML from the same projection. |
| PDF first | Fixed pages are useful for distribution, but PDF is not the editable or round-trip record. Add it after render QA and citation/redaction verification exist. |
| ZIP as the first container | A single file is convenient, but directory bundles avoid decompression in the first importer. Transfers must include the complete directory; inventory hashes detect loss or alteration. |
| Remote media in HTML | Smaller files would depend on current network/source state and could disclose reading activity. Use bounded embedded previews or explicit references instead. |
| Export-specific domain records | Duplicating versions and evidence classes would undermine one shared model. Transport schemas wrap the domain schemas and declare supported versions. |

Directory bundles can be large and less convenient to transfer. JSON and Markdown
are inspectable but are not tamper-proof. SHA-256 establishes byte integrity only,
not truth, identity, authorship, or a trusted signature. Reproduction manifests
describe dependencies; their presence does not prove an analysis can be replayed.

Export may stop because permissions changed or a configured limit is exceeded.
The user can explicitly narrow selection or change a limit, with its effect
recorded in the new snapshot. The exporter must not silently substitute a lower
quality preview, omit evidence, or change an analysis method to fit.

## Validation and follow-up

This decision is derived from the existing requirements and architecture, not a
measured performance or interoperability result. Documentation checks cover local
links/anchors, requirement references, code fences, and whitespace. A16 and A17
remain unexecuted runtime acceptance scenarios.

The #22 implementer must bind the contract to merged #11/#16/#18/#19 services,
publish the versioned schema and default resource limits, implement both report
renderers and transactional bundle import/export, and execute the focused checks
in the contract. Completing this ADR does not complete #22 or reserve that runtime
work. Revisit the decision when PDF, archives, signing, or new required schema
capabilities are proposed.
