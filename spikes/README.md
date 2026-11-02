# Renderer spikes for decision D01

Evaluation code for [ADR 0008](../docs/adr/0008-d01-workspace-renderer.md), the
renderer part of D01 that ADR 0001 (PR #56) defers to
[#16](https://github.com/mESmaC/oia/issues/16). Two front-ends draw the same
P1 shell subset from the merged [design draft](../docs/design/workspace-ui-ux.md)
over one shared model, so the options can be compared with the same fixture,
machine, and method. Nothing here is production code, and nothing here is live:
every record is synthetic and labelled as such.

| Package | Role |
| --- | --- |
| [`workspace-shell-core`](workspace-shell-core/) | Renderer-independent shell context, route stack, local filters, settings, job tray model with supersession and interruption, bounded atomic persistence, and the synthetic case fixture. Four contract tests. |
| [`workspace-shell-tauri`](workspace-shell-tauri/README.md) | Rendering-adapter option: Tauri 2 window over the Rust-held model, bundled HTML/CSS/JS presentation layer, typed commands, strict CSP. |
| [`workspace-shell-egui`](workspace-shell-egui/README.md) | Rust-native option: the same shell drawn with egui/eframe. |

This directory is its own Cargo workspace so that a future root workspace does
not absorb it. Its `Cargo.lock` is spike-local and is not the shared workspace
lockfile. Building needs network access the first time to fetch `tauri` and
`eframe`. No Node, npm, or bundler is used anywhere.

The persisted shell-state file format is throwaway. It is not the schema
proposed for `oia-app`, whose `WorkspaceShellState` in PR #56 remains the
contract to implement against once P0 merges. Note also that `persist::load`
checks file size and schema version, while re-validating `ShellState` mutation
invariants on load (clock within interval, `reading_text_px` range, route-stack
limit, and instrument availability) is a known gap of this throwaway format.

## Commands

```sh
cd spikes
cargo +1.99.0 fmt --all -- --check
cargo +1.99.0 clippy --workspace --all-targets --locked -- -D warnings
cargo +1.99.0 test --workspace --locked
cargo +1.99.0 run --release --locked -p oia-shell-spike        # Tauri
cargo +1.99.0 run --release --locked -p oia-shell-spike-egui   # egui
```

State files: `%APPDATA%\org.oia.shell-spike\` and
`%APPDATA%\org.oia.shell-spike-egui\` (Windows). Delete `shell-state.json` to
start fresh. Each launch appends a line to `startup-log.jsonl` with the
milliseconds from process start to the UI's first ready call (Tauri) or first
frame (egui).

## Method (measured 2026-11-02 UTC)

Machine: Windows 11 Home 10.0.26200, Intel Core Ultra 7 265F, 64 GB RAM, NVIDIA
GeForce RTX 5070, WebView2 runtime 156.0.4314.58. Toolchain: rustc 1.99.0;
`tauri` 2.12.5 and `wry` 0.57.1; `eframe`/`egui` 0.37.0 with the default `wgpu`
renderer (`egui-wgpu`; resolved versions in `Cargo.lock`). Release profile: `strip`, thin
LTO, one codegen unit. Source: this branch at the PR head.

A PowerShell script launched each binary, waited for a main window wider than
200 px, waited a fixed settle time, sent key sequences with
`WScript.Shell.SendKeys` and mouse clicks through `user32`, captured the window,
then summed `WorkingSetSize` over the process tree (the app and every
descendant, found through `Win32_Process` parent ids) and listed established
TCP connections owned by that tree with `Get-NetTCPConnection`. Five launches
per binary, warm: the binaries and the WebView2 runtime were already cached by
the OS. No cold-boot timing. The same fixture, window size, and scenarios were
used for both.

## Results

| Measure | Tauri 2 adapter | egui/eframe |
| --- | --- | --- |
| Release binary | 8,948,224 bytes | 14,620,160 bytes |
| Process start to UI ready / first frame, 5 launches | 305 to 321 ms | 383 to 465 ms |
| Process start to main window, 5 launches | 125 to 269 ms (first launch slowest) | 451 to 869 ms (first launch slowest) |
| Working set, main process | 26.6 to 32.7 MB | 156.7 to 208.3 MB |
| Working set, whole process tree | 361 to 382 MB (app + 6 WebView2 processes) | same as main (1 process) |
| Established outbound TCP connections during a 4 s idle run | 2, owned by the WebView2 browser process, to a Microsoft address on 443 (no PTR record), not initiated by the page | 0 |
| Contract tests (shared core) | 4 passed | 4 passed |

Both front-ends completed the same scenarios under automation: fresh start;
restore after kill with the OCR job shown as interrupted; selection with
coordinated inspector, context strip, readout, and brackets; search with scope,
mode, count, pagination, and coverage line; a stale search result reported as
superseded and not applied while the current result is shown; clean evidence;
focused reading; settings; route back. Screenshots are in each package's
`screenshots/` directory.

What the numbers do and do not say: they characterise two spikes on one warm
machine. The egui binary is larger because it carries its renderer and fonts;
the Tauri binary is smaller because the renderer is the system WebView, whose
six processes account for most of the 360 MB tree. The outbound connections
from the WebView2 browser process are a property of the runtime, not of the
page: the page's CSP is `default-src 'self'` and the UI has no network client.
Passing `--disable-background-networking`, `--disable-component-update`,
`--disable-domain-reliability`, `--metrics-recording-only`, `--no-pings`, and
extra `--disable-features` entries through Tauri's `additionalBrowserArgs` did
not remove them (three runs; the flags were confirmed on the browser process
command line). That experiment is not kept in the shipped configuration.

## What neither spike establishes

No screen-reader session, no Linux or macOS build, no cold-boot timing, no
bright-imagery reading test (A02), no measurement of a real evidence load.
Neither spike is A01 or A02 acceptance; both are evidence for a renderer
decision that belongs to the project owner.
