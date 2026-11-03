# AGENTS.md

Instructions for coding agents working in Open Intelligence Agency (OIA). Read this file before changing code, documentation, configuration, or assets. It applies throughout this repository; narrower instructions may add requirements for their directories. Tool-specific instruction files should refer here rather than duplicate these rules. Explicit task instructions and higher-priority harness instructions take precedence.

OIA is a Rust-based intelligence and research suite with selected algorithmic properties specified and proved in Lean. UAP research, public documents, geographic events, media forensics, personal sensors, news, and forecasts share one investigation and evidence model. The visual direction is a futuristic military terminal with refined glass surfaces and clear, usable information.

## 1. Start with the actual repository

OIA defines a Rust workspace (`Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` pinned to `1.99.0`, dual-licensed under `MIT OR Apache-2.0` via `LICENSE-MIT` and `LICENSE-APACHE`) with architecture decision records in `docs/adr/`. See `Cargo.toml` for workspace members and `README.md` ("Current status") for which crates carry implemented kernels versus boundary stubs awaiting split P0–P6 implementation PRs. Inspect the current tree, ADRs, and configuration before running commands or describing capabilities.

Read the documents relevant to the task:

- [Product specification](docs/spec.md): scope, user workflows, and stable requirement IDs.
- [Architecture and data contracts](docs/architecture.md): component boundaries, evidence records, time/geometry, adapters, storage, and verification.
- [Delivery and verification](docs/delivery.md): proposed milestones, acceptance scenarios, release gates, and decision register.
- [GitHub implementation backlog](docs/backlog.md): open work, phase epics, and the implementation Project.
- [Selective reuse inventory](docs/prior-art.md): inspected God's Eye View source paths, revision, lessons, and attribution.
- [Exploration archive](docs/spec-draft.md): historical context only; the specification set supersedes it.

Check `git status` before editing. Preserve existing user changes and work from other sessions. Do not reset, overwrite, stash, delete, or change branches to make the workspace look clean. Read an existing implementation and its tests before replacing it.

For implementation work, identify the requirement IDs, applicable acceptance scenarios, relevant open decisions, and intended end-to-end behavior. Resolve ordinary implementation choices within the authorized task and record material decisions. Do not silently settle major open choices by introducing an unrelated framework or infrastructure dependency.

### Check open work and mark what you are working on

GitHub issues and the [implementation Project](https://github.com/users/mESmaC/projects/4) are the source of truth for work ownership and progress. Checking and updating them is part of the task, not an optional handoff step.

1. Before starting work, inspect the repository's open issues, relevant Project statuses, dependencies, assignees, recent comments, and linked pull requests. Follow the user's requested scope; do not select unrelated backlog work merely because it is available. Use [the backlog index](docs/backlog.md) to locate the relevant records, then read their current GitHub state.
2. Identify the specific issue or work package being addressed. Before implementation, mark its Project item **In Progress** and record a concise claim on the issue identifying the agent/session, intended scope, and branch or worktree when applicable. Use the appropriate GitHub assignee when available, but distinguish sessions sharing the same account. For substantive work without a matching issue, create a scoped issue and add it to the Project when GitHub tracking is within the authorized task.
3. Recheck ownership before claiming work. Do not overwrite another session's claim, remove its assignee, or duplicate an active implementation. Resolve overlap through an explicit handoff or a separate, clearly bounded task. Do not mark an entire phase or unrelated implementation package active for a small documentation change.
4. Keep the issue and Project current as scope, dependencies, or delivery change. Record material progress, blockers, and the next action; link the relevant commit or PR. If work pauses or is handed off, state who owns the next action. Return abandoned, unclaimed work to **Todo**; keep unfinished work open. If the Project has no blocked status, record the blocker on the issue rather than inventing a status or marking it Done.
5. Mark **Done** and close the issue only after its completion criteria, applicable checks, and required delivery are satisfied. A PR awaiting merge is not Done; for an explicitly authorized direct push, verify the remote commit first. Record completion evidence and remaining limitations. Completing one child issue does not complete its parent epic or every requirement it references.
6. Treat split or partial work explicitly: track remaining scope in open linked issues and preserve requirement and dependency references. Close phase epics only when all required work and integrated acceptance checks are complete.
7. If GitHub access is unavailable, state that the claim/status update could not be recorded, keep a local handoff with the issue, scope, and intended status, and reconcile it when access returns. Never report a successful claim or status change without verifying it. This fallback does not authorize taking over work already claimed by another session.

### Coordinate concurrent sessions

Several sessions may work at once in separate worktrees under one GitHub account, so the assignee does not identify an owner; the claim comment does.

1. **Claim only what you are starting.** Claim a package when implementation begins, not every package you expect to reach. A claim on a phase epic records coordination of that phase; it does not reserve its child issues. A claim may cover a named slice of an issue: state the slice and what remains unclaimed. Other sessions may claim disjoint slices of the same issue after reading every claim comment on it. The Project item stays **In Progress** while any slice is claimed; the comments, not the status, define who owns what. Release claims you are not actively working on with a comment, and return the item to **Todo** once no slice remains claimed.
2. **Comment first, then status.** Post the claim comment before moving the Project item to **In Progress**. Post it with `gh issue comment <n> --body-file <file>` (or a single-quoted here-string in PowerShell, which does not expand backtick escapes), then re-read the posted comment and confirm the session, branch, worktree, and scope are intact before changing the status. Treat either signal as ownership if the other is missing, and ask on the issue rather than assume the work is free.
3. **Recheck before and after claiming,** especially after waiting on user input or a long task. After posting, re-read the issue's comments. If an earlier claim covers overlapping scope, judged by comment creation time rather than edit time, withdraw yours in a comment before doing any work.
4. **Declare shared files.** List in the claim any shared files you expect to change beyond the package's own module: the workspace manifest, `Cargo.lock`, toolchain pins, `docs/spec.md`, `docs/architecture.md`, `AGENTS.md`, and scripts. Keep those edits in minimal, separate hunks without reordering or reformatting. Rebase onto current `main` before opening a PR. On a `Cargo.lock` conflict, take `main`'s version, then run an ordinary Cargo build or check without `--locked` so that only your manifest changes are added; do not delete the lockfile or run `cargo update` unless the task calls for it, and review the lockfile diff in the PR. ADR numbers in `docs/adr/` are sequential and therefore collide easily. Name the number you intend to use in your claim. If an earlier merge takes that number, renumber your ADR and its references.
5. **Do not copy unmerged prerequisites.** When a package depends on another session's unmerged branch, say so in the claim. Either base the PR on that branch and state the stack, or wait for it to merge.
6. **Hand off explicitly.** Request a handoff on the issue. The owning session, or the user, releases the work in a comment before anyone else starts. Inactivity alone does not make a claim stale; if the owning session appears to have ended, ask the user.
7. **Review sessions stay out of implementation.** A session assigned to review records its role on the relevant epic and comments on issues and PRs. It does not push to another session's branch or claim that session's packages. Because every session uses the same account, GitHub allows only `COMMENT` reviews, not approvals or change requests. Begin each review with the reviewing session's identity and mark each finding blocking or non-blocking. The user decides whether to merge, or authorizes a session to merge.
8. **Deliver finished work as a PR.** When a claimed issue or slice is complete and its local checks pass, commit on the task branch, push, and open a PR against `main`. If it depends on another unmerged branch, base it on that branch and state the stack. Link the issue in the PR description (`Refs #n`; use `Closes #n` only when the PR completes the issue) and post the PR link on the issue. The issue stays **In Progress** while the PR is open. Then monitor the PR: respond to review comments and requested changes, push fixes to the same branch without force-pushing over reviewed commits, and re-run the checks you reported. Do not merge your own PR unless the user authorizes it. After merge, update the issue and Project (section 1) and clean up the worktree (section 10).

Check active work with these commands (POSIX shell or Git Bash):

```sh
gh project item-list 4 --owner mESmaC --format json --limit 200 --jq '.items[] | select(.status=="In Progress") | .content.number'
gh issue view <number> --comments
gh pr list --state open
```

Windows PowerShell 5.1 strips embedded double quotes from native-command arguments, so escape them there:

```powershell
gh project item-list 4 --owner mESmaC --format json --limit 200 --jq '.items[] | select(.status==\"In Progress\") | .content.number'
```

## 2. Preserve evidence and analytical quality

OIA's central contract is that a user can inspect how a result follows from its sources. A successful request, attractive visualization, or passing test is not a reason to weaken that contract.

- Preserve original retained bytes. Create separate artifacts for extraction, cleanup, enhancement, overlays, prediction, simulation, and recreation.
- Preserve source IDs, exact versions, citation anchors, processing history, and review status through every transformation and export.
- Keep observations, source assertions, assumptions, inferences, forecasts, and generated material distinct. Feeding an inference into another tool must not promote it to an observation.
- Never convert missing fields to plausible measurements. Zero, a default altitude, an interpolated position, a placeholder frame, or a stale held value must not masquerade as an observation.
- Keep source acquisition, capture, event, publication, revision, retrieval, and analysis times distinct. Preserve uncertainty and the original timezone/time reference.
- Represent coverage gaps, partial processing, failed acquisition, empty results, and observed absence separately.
- Preserve disagreements, counterexamples, corrections, and unsuccessful forecasts. Do not improve apparent results by discarding inconvenient evidence or rewriting past predictions.
- Do not silently lower resolution, precision, context, source coverage, or analysis quality, or switch model/provider/method, to make a task succeed. Respect explicit settings and documented source/model limits; report unsupported requests and incomplete results honestly.
- Bounded requests, retention, sampling, model budgets, and cancellation are required operational controls. Their effect on coverage or output must be visible. A documented partial result is acceptable; an apparently complete result with hidden omissions is not.
- Quality-reducing options must state their effect, have a defined scope, and be recorded with the result. A display-quality setting must not change scientific calculations or original evidence.

Hash equality demonstrates byte integrity, not truth or authenticity. Multiple articles repeating a source are not independent corroboration. Correlation is not causation. Model confidence is not a calibrated probability unless evaluated as such.

## 3. Be precise about claims and verification

Keep these categories explicit in code comments, UI, documentation, reports, and handoffs:

- **Implemented:** An execution path exists and its limitations are stated.
- **Tested:** Name the executed tests or reproduction, environment, and outcome.
- **Measured:** Record data, hardware, configuration, date, software revision, and method.
- **Derived/modelled:** State the inputs, assumptions, calculation, and uncertainty.
- **Simulated:** Identify generated or fixture inputs and their difference from observations.
- **Proposed/target:** A design or performance objective, not a shipped feature or result.
- **Proved (model):** Cite the Lean theorem, assumptions, checked toolchain/revision, and what model it covers.
- **Implementation correspondence established:** Explain and cite the actual bridge from the theorem to the Rust implementation. Tests alone do not establish a formal implementation proof.

An adapter declaration is not a working provider integration. Hardware classification is not tested hardware support. A catalog entry is not access to historical data. A smooth visualization is not a measurement. Do not label a figure "measured" or a component "verified" without the corresponding evidence.

### Lean discipline

OIA uses Lean for selected, explicitly scoped algorithmic properties; it does not require a theorem for every empirical observation, ordinary bug report, or prose change.

For a new proof-backed component, record its specification, theorem, assumptions, input domain, Rust items/revision, correspondence method, test fixtures, and numerical limitations. For changes to an already covered algorithm, update the model/proof and correspondence evidence in the same change, or explicitly withdraw the affected claim until it is re-established.

Do not weaken a theorem, remove a proof from verification, change its domain covertly, or introduce `sorry`, `admit`, or an unproved project-specific axiom to get a green result. Disclosed mathematical assumptions must remain visible in the claim; a computational proof shortcut must not enlarge the trusted base without explanation.

Proofs over ideal arithmetic do not establish floating-point correctness automatically. Timing, source truth, physical models, detector quality, and forecast accuracy require independent reference data or empirical evaluation. Report those separately.

Lean is a local development and verification tool by default. Do not add a Lean runtime, invoke Lake from production startup/build hooks, or link generated proof runtime into Rust binaries as an incidental dependency. Any deliberate change to this boundary requires an explicit architecture decision within the task's scope.

## 4. Keep one domain model and clear ownership

Follow the dependency direction in the architecture: presentation and adapters depend on application/domain contracts; the domain core does not depend on a renderer, provider, database engine, model service, or vendor SDK.

- Put behavior with its owner. Do not duplicate source registries, evidence stores, time conversions, credential handling, job queues, or forecast records in individual feature modules.
- UI, CLI, API, and model-tool interfaces must use the same application services and authorization rules.
- Keep commands distinct from read-only queries. Retrying a query must not initiate device control, publication, or another mutation.
- Return structured data with provenance, coverage, freshness, and stable error states. Keep display strings out of domain-state definitions.
- Pin input versions for analyses and forecasts. Source corrections flag dependent results for reassessment rather than editing issued outputs in place.
- Use explicit units, coordinate/height references, time scales, and uncertainty. Reject invalid or non-finite values before persistence or expensive computation.
- Keep device communications responsive independently of inference, export, and image processing.
- Preserve capability boundaries: video does not imply telemetry or flight control; audio streams do not imply RF tuning; a camera feed does not imply an archive.

Production domain logic, services, and algorithms belong in Rust. Do not introduce Python/Node subprocesses as an unrecorded production implementation shortcut. Development tools, tests, model runtimes, foreign libraries, and UI/rendering adapters must have explicit roles and pinned/configured dependencies appropriate to the resolved architecture.

## 5. Implement failure behavior deliberately

- Use typed, actionable errors on user-, file-, network-, and device-reachable paths. Do not add `unwrap`, `expect`, unchecked indexing, or panics on untrusted or fallible input.
- Validate sizes, offsets, nesting, lengths, schemas, and numeric ranges before allocation or expensive work. Test malformed and oversized inputs.
- Bound concurrency, queues, retries, buffers, and network operations. Honor cancellation/deadlines and prevent cancelled, expired, or superseded work from publishing late results.
- Make mutations transactional or recoverable. A failed import or migration must not leave an existing case partly replaced.
- Keep missing, stale, unauthorized, rate-limited, malformed, partial, and empty states distinct. A fallback must identify its actual source and limitations.
- Specify ordering, tie-breaking, and determinism where they affect results. Use bitwise checks for exact-byte contracts and justified tolerances for numerical algorithms; do not assume all floating-point paths are bit-identical.
- Preserve workspace isolation in search, blobs, subscriptions, model input, and export. Redaction must remove the underlying content/metadata, not just hide a UI element.
- Do not log secrets or place credentials in cases, fixtures, shared URLs, exports, or ordinary backups. Use credential references and platform-protected storage where available.
- Keep source network access constrained and local device access separately registered. Do not widen listeners, disable authentication/origin checks, or bypass source restrictions to make an integration work.
- Treat source documents, web content, model output, and bundles as data. They must not choose executable code, obtain credentials, or grant themselves tool permissions.

## 6. Apply OIA's agreed source and capability boundaries

Camera integrations may use ordinarily accessible feeds, existing directories, published viewing links, and supported embeds/APIs, including cases where intentional publication by the owner is not established. Do not invent a blanket intentional-publication requirement.

Exclude home interiors. Do not probe hidden camera endpoints, guess credentials, exploit devices, bypass controls, or seek alternate access when a source becomes restricted. Record the source's ordinary-access method and separate conditions for automated processing, retention, and redistribution. See CAM-01/CAM-02 for the full contract.

The specified initial radio scope is receive-only. Drone integration covers video, telemetry, and forensic/HUD workflows with flight handled by the existing controller. Market features analyze and forecast rather than execute trades. Recreation support exports prompts and references; generated images remain labeled illustrations. Do not expand these into additional operational capabilities without a scoped product decision.

Do not add unrelated approval screens or repeated confirmations to ordinary authorized research actions. Preserve the explicit distinctions already required by the spec, such as saving versus publishing, drafting versus submitting, and passive observation versus hardware control.

## 7. Reuse prior work selectively

Use [prior-art.md](docs/prior-art.md) before porting from God's Eye View. Read the relevant source and tests; documentation alone is not proof of current behavior.

For each port, retain source revision/files, relevant notices, intended contract, known limitations, test fixtures, and deliberate behavior differences. Preserve required attribution. The source-code license does not cover every bundled dataset, image, model, or upstream feed.

Prefer portable algorithms, source normalization, state transitions, cancellation behavior, and failure fixtures. Do not copy the entire application layout or keep legacy compatibility solely for parity. Renderer defaults, synthetic camera substitutes, or approximate heights must not become observed evidence in OIA.

Differential tests against prior work help detect regressions; independently validated references determine whether a legacy behavior should be preserved. Do not access credentials or modify the reference checkout as a side effect of reading or porting it.

## 8. Verification commands and scope

The repository defines a 15-crate Cargo workspace (`Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` `1.99.0`). The Lean 4.35.0 package in `proofs/` checks model proofs and the first Rust cutoff correspondence fixture; see `proofs/README.md`. Check for actual manifests, lockfiles, toolchains, scripts, and CI configuration before issuing build commands. Do not copy Kadmium crate names, versions, ports, or test paths into this repository.

### Local automation and functionality first

Run build, lint, verification, packaging, and authorized release/deployment steps locally by default to avoid hosted CI/CD costs. Do not add, enable, or trigger GitHub Actions or other hosted CI/CD jobs unless the user explicitly requests them. References to CI or release gates in the specification describe checks to satisfy locally; they do not require a hosted runner. Prefer reproducible local commands or scripts using the pinned toolchains, and record what ran, the revision, environment, and results in the handoff or PR. Running locally does not expand authorization to publish or deploy.

Implement working functionality first. Do not make test scaffolding, test-driven development, broad test matrices, or coverage targets prerequisites for ordinary feature work. The default preference is to add no new test cases. Verify the actual behavior with focused local execution or inspection appropriate to the change.

Add only the small number of targeted test assertions needed to establish a critical contract or catch a concrete failure that ordinary verification would miss. Examples include evidence preservation, access isolation, destructive recovery, nontrivial numerical correctness, and the Rust-to-Lean correspondence required for a proof claim. State what risk or requirement each added test addresses. Avoid redundant cases, tests that merely mirror implementation details, and large speculative suites. A compact fixture with relevant assertions is preferable to many repetitive cases.

This preference does not remove input validation, required acceptance evidence, existing meaningful tests, or Lean proof obligations. Assertions in test code must not become production panics on fallible or untrusted inputs. Run applicable existing checks, and do not delete tests or weaken their assertions just to reduce the test count.

For documentation-only work, check the diff, relative links/anchors, requirement references, code fences, and consistency with the canonical spec. `git diff --check` does not include untracked files; inspect new files as well. Do not report unrun product tests as passing.

Once a Rust workspace and lockfile exist, the baseline checks should be:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Use the pinned project toolchain when present. Establish and review the lockfile explicitly during bootstrap; do not remove `--locked` to hide unexpected resolution changes. Run the repository's actual feature/platform matrix when relevant rather than guessing `--all-features` is valid.

While developing, implement and exercise the changed behavior locally; run applicable existing focused tests and add assertions only where justified above. Before delivering runtime changes, run the applicable baseline and relevant acceptance/integration checks locally; report unavailable platforms, hardware, credentials, or tools precisely. Documentation-only changes do not require unrelated runtime or hardware suites.

When a Lean package exists, run its documented checks from its actual directory. Pin its toolchain and make proof placeholders fail the validation gate. Never cite a hypothetical command or nonexistent theorem audit as an executed check. Rust changes covered by proofs also require the appropriate correspondence tests.

When targeted assertions are necessary, select the relevant behavior or failure mode: malformed input, incomplete coverage, clock/coordinate boundaries, cancellation, stale completions, permission isolation, or original preservation. This list is not a mandatory test matrix for every change. Use deterministic fixtures for repeatable local verification; live provider/hardware checks must be identifiable and separately configured.

For forecasts, enforce point-in-time data, fixed outcome definitions, chronological evaluation, and preserved failed predictions. For detectors, report operational false alerts, event recall, latency, and coverage on held-out data. Do not change seeds, thresholds, samples, or baselines simply to improve reported results.

Never weaken or delete a meaningful test to obtain a pass. Explain an intentional contract change and update its specification and tests together. Once required checks pass, repeat or broaden them only when changes, failures, or unresolved concerns justify it.

## 9. Product writing and UI changes

Use plain, precise language and consistent states across UI, logs, interfaces, and docs. Distinguish proposed, processing, partial, stale, reviewed, and unavailable states according to their contracts. Avoid marketing claims about certainty, accuracy, access, or security that are not established.

Keep the cinematic military aesthetic original and functional: dark canvas, refined glass, restrained accents, measured typography, meaningful instrumentation. Preserve keyboard access, readable text, reduced effects, and clear uncertainty. Do not fill screens with fake telemetry, pretend access labels, decorative alerts, or simulated activity presented as live.

Keep implementation details out of ordinary user flows unless they explain an actionable limitation or choice. Show detailed provenance, models, parameters, and calibration in inspectable panels and exports. Do not hide them entirely to simplify the UI.

Update relevant specifications, adapter documentation, compatibility lists, and decision records with the implementation. Keep the exploration archive historical; do not let it become a competing source of current requirements.

## 10. Delivery and repository hygiene

Complete the user's authorized task through implementation and relevant verification. Do not stop at a plan, scaffold, or partially functioning path when the task requires a working result. Claimed backlog work is delivered as a PR under rule 8 of "Coordinate concurrent sessions" in section 1. Otherwise, do not broaden a local authoring task into an unsolicited commit, push, PR, external message, or deployment; merging, deploying, and publishing still require explicit authorization.

Keep edits focused. Avoid unrelated refactors, formatting churn, generated artifacts, caches, and local credentials. Format changed code without accidentally rewriting unrelated modules; inspect the final diff.

When version-control delivery is within scope:

- Use the appropriate task branch and existing checkout/worktree arrangement. Do not commit directly to the default branch, silently move an active session, or discard another task's changes.
- Verify the intended base and dependency history before opening a PR; do not smuggle another unmerged change into a default-branch PR.
- Use concise conventional commit subjects. PR descriptions must explain the problem/result, requirement IDs where useful, evidence/quality impact, and exact validation performed. State limitations and skipped checks honestly.
- Do not force-push over reviewed work or merge unless explicitly authorized. Address review findings without lowering evidence quality or weakening checks.
- Use available harness linking/watching mechanisms for authorized PR work. Follow their lifecycle rather than adding polling loops or promising background monitoring that is not running.
- Clean up task-owned worktrees after their PRs merge, following the mandatory checks below. Never remove the main checkout or another session's worktree.

### Post-merge worktree cleanup is required

After a PR is confirmed merged, remove the worktree created or assigned for that task and its task-local branch when safe. Cleanup is part of finishing the PR workflow, not an optional follow-up. Do not leave completed worktrees behind merely because the code has merged.

Before removal:

1. Confirm the PR is merged into its intended base. A closed but unmerged PR does not qualify.
2. Confirm the worktree belongs to this task and is not in use by another session. Identify the main checkout with `git worktree list`; never remove it.
3. Require `git status --porcelain` in the worktree to be empty, including untracked files. Confirm all local work is accounted for by the merged PR and that no additional commits were made after the merged PR head. Account for squash/rebase merges rather than relying solely on commit ancestry.
4. Resolve and verify the absolute worktree path before removal. Run `git worktree remove` from the main checkout or another retained checkout, not from inside the worktree being removed. Do not use forced removal to bypass these checks.
5. After successful removal, delete only the task-local branch if no other worktree or active task uses it. Forced local branch deletion is permissible for a confirmed squash/rebase merge only after the preservation checks above; leave remote branch deletion to the repository's policy or explicit task instructions.

If cleanup would lose work or disrupt another session, retain the worktree and state the exact reason and remaining work in the handoff. Do not discard changes to satisfy cleanup. Apply the same ownership and preservation checks to scratch worktrees when their review or testing task finishes.

A final handoff must state what changed, why, which checks actually ran, and any material remaining limitations. Link the relevant files or PRs. Keep proposals, implementation, tests, measured results, and proof claims distinct.
