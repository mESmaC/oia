# Local desktop workbench

Implemented slice of [#16](https://github.com/mESmaC/oia/issues/16), following the owner's Tauri selection in [ADR 0008](../adr/0008-d01-workspace-renderer.md). `oia-app` now opens a desktop window and connects the merged document and PNG/JPEG privacy kernels to real local files. It does not import files into a case or implement the persistent services owned by #10/#11/#14.

## Run

```text
cargo run -p oia-app --locked
```

On Windows, `.\scripts\run.ps1 run` launches the same app. `scripts/run.sh run` is the POSIX equivalent. `run PATH ...` retains the existing text-inspection behavior; the explicit spelling is `inspect PATH ...`. `search` remains the text CLI example. `oia --status` reports scope without opening a window. `cargo run -p oia-app --no-default-features --locked -- --status` works without desktop dependencies being compiled.

The pinned Rust toolchain is 1.99.0. First builds need the locked Cargo dependencies; subsequent cached builds may add `--offline`. The Windows desktop needs MSVC C++ build tools and WebView2. The UI has no npm, Node, development server, or network provider requirement. Linux/macOS need the usual Tauri system dependencies and have not been validated by this slice.

## Available workflows

1. **Document reader:** select a UTF-8 file through a native dialog, read line passages, search exact case-sensitive text, and select a match to focus its original source line. Byte offsets are zero-based/end-exclusive and remain anchored to the opened bytes. Pagination presents 100 passages at a time without changing extraction coverage.
2. **File privacy:** select a PNG/JPEG, inspect container classifications and coverage, choose removable metadata categories, and save a cleaned copy through a native dialog. The original stays unchanged. The existing kernels reinspect the output and check their exact container-preservation contracts before the copy is published. This is not full image decoding, content anonymization, or proof that every identifying field was removed.
3. **Presentation:** keyboard-accessible native controls, Ctrl+O for a document, Ctrl+F for document search, visible focus, focused reading, and reduced effects. There is no animation or sound. Controls/coverage use labels in addition to color. Source content is assigned as text, never HTML.

One document and one image snapshot can be held at a time. Switching views retains them and the displayed search in memory; closing the process discards them. UI settings are also session-only. Reopening a changed source explicitly replaces the snapshot. A cancelled or failed open preserves the previous snapshot. No file is automatically reopened after restart.

## Boundary and limits

`src/local_tools/` is a stateless I/O/DTO adapter over the merged kernels: bounded reads, immutable byte snapshots, SHA-256, kernel calls, typed errors and safe output staging. It introduces no case store, job service, query index or alternative extraction/cleanup algorithm. #14 can adopt these functions behind its shared service contracts. `src/desktop.rs` is the Tauri/native-dialog adapter; it owns the transient single-operation gate and publishes results only after a cancellation check. `ui/` draws Rust responses and sends intents; its local state is presentation (tabs, reading pages, focus, checked removal choices), not a parallel evidence model.

The application-specific transport envelope is `oia-local-tools/1`; Rust serializable structs are the schema, and JavaScript does not declare a duplicate domain type model. These are transient file-tool results, not persisted evidence envelopes or source-version IDs. A SHA-256 identifies bytes only; a local path is not a stable citation or source record. Publication, capture and event times are not invented from filesystem timestamps.

| Operation | Bound and visible effect |
| --- | --- |
| Text read | 2 MiB; oversized input rejected, never silently truncated |
| Extraction | 10,000 passages; remaining bytes explicitly reported as partial |
| Search | 512 UTF-8 query bytes, 200 matches, exact non-overlapping matches within LF-delimited passages; a reached budget remains partial |
| Image read/output | 16 MiB; bounded regular-file reads and no image decoding |
| Image container | 4,096 chunks/segments, 2 MiB metadata, 32,768 pixels per axis, 100 million pixels total |
| Processing | One active worker; 30-second cooperative deadline after dialog selection; cancellation checked inside kernels and at publication |

File I/O and OS dialogs are not preemptible. Cancel requests prevent subsequent publication but may need the user to close an open dialog, or wait for an OS read to return. No fabricated progress percentage is displayed. Busy commands are rejected rather than queued. Cancellation and commit share the same lock: once an export has committed, a later cancellation does not undo it. These workers are not durable jobs and do not claim restart recovery.

The reader escapes hidden controls and bidi formatting for display only; exact source bytes/offsets stay intact. Text and image tools do not silently substitute a parser, infer missing metadata, or convert an unsupported image profile into a successful cleanup. In particular, many camera JPEGs with ICC/MPF/gain-map metadata are outside the conservative profile. See [PNG privacy](png-privacy.md) and [JPEG privacy](jpeg-privacy.md).

Exports stage in a temporary file beside the chosen destination, flush it, then use no-clobber publication. Existing destinations, including the source and links to existing files, are refused. A collision after staging is also refused. Failed/cancelled staging is removed by the temporary-file owner. A process/OS crash may leave a temporary file; no case-store crash recovery or directory durability claim is made.

## Runtime privacy

There is no localhost HTTP listener, remote web content, JavaScript file/network/shell plugin, or source-text execution. Tauri IPC is available only to the bundled main window. The CSP forbids remote resources, frames, objects and form submission. Native dialogs select the files; the frontend cannot supply an arbitrary path to a Rust command.

The Windows **WebView2 runtime may contact Microsoft independently of the page**. ADR 0008 observed background HTTPS connections in the renderer spike; this work does not claim to suppress them or establish network silence. The app's overview discloses this behavior. That observation belongs to the spike, not a new measurement of this build.

## Requirements and verification scope

This supplies bounded local-file portions of CASE-04/05, DOC-02/04, PRIVACY-01 and UX-01–UX-05. It exercises relevant subsets of A02/A03/A18. Persistent cases, durable cancellation/retry/restart, workspace permissions, provenance envelopes, PDF/OCR, remote acquisition, semantic search, AI answers, reports and integrated A01–A03/A18 acceptance remain open. No Lean theorem or Rust/Lean correspondence claim is introduced.

Three compact integration tests address concrete risks: source mutation changing an already-open search/anchor; hidden limits, malformed input and cancellation; and destructive export/late destination collisions plus output reinspection for PNG and JPEG. One desktop gate test verifies rejection of overlapping work and publication after cancellation. The desktop adapter also needs a manual interaction walkthrough; kernel tests alone do not establish a working app.

Validation results are recorded with the PR after local checks and the desktop walkthrough finish.
