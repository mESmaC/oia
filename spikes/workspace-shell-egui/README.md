# Workspace shell spike: egui/eframe (Rust-native comparison)

The Rust-native half of the renderer comparison in
[ADR 0008](../../docs/adr/0008-d01-workspace-renderer.md). It draws the same
P1 shell subset as the [Tauri spike](../workspace-shell-tauri/README.md) over
the same shared model ([`oia-shell-core`](../workspace-shell-core/)), so the
two can be measured the same way. Fixture data only; nothing is live.

## What it shares and what differs

Shared, from `oia-shell-core`: shell context and route stack, instruments and
their availability, settings, job registry with supersession and interruption,
bounded atomic persistence, and the synthetic case fixture (geometry, objects,
passages, report, export preview, timeline marks). The four contract tests run
against that crate, so they cover this spike too.

Drawn here with `egui` 0.37 through `eframe` (default `wgpu` renderer via
`egui-wgpu`, default fonts, AccessKit enabled by eframe's default features but
not tested):

- Full-bleed black canvas with the case geometry (rings, ridge sketch, region,
  track, wedge, observer, selection brackets), floating panels for the case
  card, rail, context strip, clock, inspector, timeline, dock, job tray, and
  readout, plus the fixture strip and a notice area.
- Library, Report, Export, Jobs, and Settings panes with the same fixture
  content and the same disabled actions.
- Keyboard: `Ctrl+1..9`/`Ctrl+0`, `Ctrl+K`, `Ctrl+Shift+E`, `Ctrl+Shift+F`,
  `Alt+Left`, arrow keys to scrub the clock when no text field has focus,
  `Escape`. Chords are read from per-event modifier state so a modifier
  released in the same frame still counts.
- Fixture workers: the OCR job advances on a timer and ends partially
  succeeded; retrying the acquisition job fails again; search runs on a thread
  with the same simulated latency and goes through `JobRegistry::finish`, so a
  stale result is superseded and shown as "not applied".
- Persistence to `%APPDATA%\org.oia.shell-spike-egui\shell-state.json` with the
  same restore and interruption behaviour (unreadable files are moved aside to
  `shell-state.json.unreadable-<unix>` before defaults are saved); one line per
  launch in `startup-log.jsonl` with the milliseconds to the first frame.

Where egui could not express the design as written, the spike shows the
nearest native equivalent rather than hiding the gap:

| Design rule | egui spike |
| --- | --- |
| Glass with 14 to 20 px backdrop blur (§7.2) | Translucent fill only. egui has no backdrop-blur primitive; a custom render pass would be needed. Reduced transparency swaps to the opaque tokens as designed. |
| Hatched fill for region-only evidence (mockup) | Translucent amber fill with a dashed outline. egui has no pattern fill. |
| Tracked uppercase monospace labels (§7.3) | Uppercase monospace at 11 px with `RichText::extra_letter_spacing(0.9)` (`≈0.08 em`). |
| Unknown position listed outside the geometry (§7.6 v0.2) | A "Not plotted" row of buttons above the canvas; no marker is drawn. |
| Responsive layout down to 320 px (§3 v0.2) | Fixed-position panels for a 1400 × 840 window and larger; not attempted. |

## Running

From `spikes/`:

```sh
cargo +1.99.0 run --locked -p oia-shell-spike-egui              # debug
cargo +1.99.0 run --release --locked -p oia-shell-spike-egui    # release, as measured
```

Delete the state file above to start fresh. Measurements and the comparison
table are in [`spikes/README.md`](../README.md) and the ADR.
