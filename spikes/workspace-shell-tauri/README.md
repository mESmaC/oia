# Workspace shell spike: Tauri 2 adapter over a Rust-held shell context

The rendering-adapter half of the renderer comparison in
[ADR 0008](../../docs/adr/0008-d01-workspace-renderer.md). A Tauri 2 window
hosted by a Rust process renders a bundled HTML/CSS/JS presentation layer over
the shared model in [`oia-shell-core`](../workspace-shell-core/). The
Rust-native counterpart is the [egui spike](../workspace-shell-egui/README.md);
measurements and method for both are in [`spikes/README.md`](../README.md).

Everything shown is fixture data. The window title, the strip at the top of the
window, every job, and every record say so. This is not the production
workspace, not a completion of A01 or A02, and not a decision.

## What it demonstrates

- The P1 shell from [the design draft](../../docs/design/workspace-ui-ux.md) §3:
  command field, context strip, navigation rail, evidence canvas, inspector,
  shared timeline, job tray, and view-state readout over a true-black canvas
  with floating glass instruments, using the draft's §7 tokens (v0.2: essential
  labels at 11 px or larger; unknown positions listed outside the geometry in a
  "Not plotted" row, never drawn at an invented point).
- **Shell context owned by Rust.** Instrument, shared context, route stack,
  per-instrument local filters, settings, and query sequence live in the core
  crate. The presentation layer draws them and sends typed intents back; it
  computes no domain value. Every mutating command returns the full shell view.
- **Coordinated selection and route back** (CASE-02, UX-02): one route entry per
  selection or instrument switch; `Alt+Left` restores instrument, selection,
  local filters, clock, and timeline focus as one step.
- **Jobs with honest states** (CASE-04): reported progress only, "progress not
  reported" otherwise, partial extent carried with the result, cancel and
  retry, and supersession: a search that finishes after the query sequence has
  moved on is retained but never applied until the user opens it.
- **Restart** (A01 slice): shell context and jobs persist to one bounded JSON
  file written atomically; jobs active at shutdown come back `interrupted`; an
  unreadable file is reported, not silently replaced. The file format is
  throwaway and is not the schema proposed for `oia-app`.
- **Settings that cannot touch data** (UX-04, UX-05): reduced motion, reduced
  transparency, effects quality, clean evidence, focused reading, sound (off),
  reading text size, as typed changes.
- **Keyboard model** (design §8.1 subset): `Ctrl+K`, `/`, `Ctrl+1..9` and
  `Ctrl+0`, `Ctrl+Shift+]`/`[`, `F6`/`Shift+F6` across regions, `Alt+Left`,
  `Ctrl+Alt+1..5` inspector tabs with arrow-key roving, `Ctrl+Shift+E`,
  `Ctrl+Shift+F`, arrow keys and `Home`/`End` on the timeline, `Escape`.
- **Transport:** every response is an envelope
  `{ schema: "oia-shell-spike/1", fixture: true, outcome, coverage, data }`.
- **No remote loads from the page:** CSP `default-src 'self'; connect-src ipc:
  http://ipc.localhost; …`, the single `core:default` capability, no plugins, no
  network client, three static files with no build step. See the measurement
  notes for what the WebView2 runtime itself does on the network.

## What it does not do

- No domain records, storage, ingestion, search index, or exporter. Search is a
  case-insensitive substring match over seven fixture passages, standing in for
  the bounded kernels in `crates/oia-documents`.
- Report editing, citing, assessing, notes, sharing, and export writing are
  shown as disabled with the reason. Globe, Sky, News, and Media are listed as
  "Not in this build".
- The fixture OCR and acquisition jobs simulate waiting and nothing else; they
  are labelled `fixture` in the tray.
- No screen-reader session, no Linux or macOS run; see the ADR for limits.

## Layout

| Path | Role |
| --- | --- |
| `src/commands.rs` | Tauri commands and queries, response envelopes, fixture workers. The only window-aware module. |
| `src/lib.rs`, `src/main.rs` | Builder, entry point, re-exports of the core modules. |
| `ui/index.html`, `ui/shell.css`, `ui/shell.js` | Presentation layer. |
| `tauri.conf.json`, `capabilities/default.json` | Window, CSP, the single permission set. |
| `icons/` | Placeholder window icon for the spike only. |
| `screenshots/` | Release-build captures from the validation run. |

## Running

From `spikes/`, with the pinned Rust toolchain and, on Windows, the WebView2
runtime (present on Windows 11). No Node.

```sh
cargo +1.99.0 run --locked -p oia-shell-spike              # debug
cargo +1.99.0 run --release --locked -p oia-shell-spike    # release, as measured
```

State file: `%APPDATA%\org.oia.shell-spike\shell-state.json`. Delete it to start
fresh. Each launch appends one line to `startup-log.jsonl` in the same
directory.

Two defects were found and fixed during validation: the canvas was first drawn
before the window reached its final size (now redrawn on resize and after the
first frame), and the frontend compared job sequences against a stale shell
view (now refreshed with every job event).
