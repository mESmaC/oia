# Globe instrument fixtures

Synthetic case data for the Globe instrument of the desktop workbench ([#23](https://github.com/mESmaC/oia/issues/23), `GEO-01`, `UX-03`). See [`docs/implementation/globe-instrument.md`](../../docs/implementation/globe-instrument.md).

**Origin class: synthetic.** Every record, position, time, source and name in `synthetic-valley-case.json` was written for the design studies and this fixture. Nothing is an observation, a real account, or a real place name. The coordinates fall on real terrain only so that the globe has something to draw under them. The desktop build refuses a fixture whose `origin_class` is not `synthetic`, so nothing it draws from this directory can be mistaken for evidence.

## `synthetic-valley-case.json` (`oia.fixture.globe-case/1`)

The 1994 valley case used by the v2 design studies, with coordinates added. It exercises every geometry kind the instrument draws, and the one it must not invent:

| Record | Geometry | Why it is in the fixture |
| --- | --- | --- |
| Account A | `point` with a 150 m uncertainty radius and a reported window 21:25 to 21:55 local | A described observer position with the author's bound; the uncertainty is drawn as a ring, never as a smaller dot. |
| Account B | none; `geometry_absent_reason` | Goes to the not-plotted tray with its reason. The build rejects a record that has neither geometry nor a reason. |
| Account C | `region` ring, no time | An area claim with no centre. Drawn as a dotted fill; no point is derived from it. |
| Weekly article | `approximate_centroid` with a 3 km radius and a calendar date only | A gazetteer-style centroid, labelled as such, with a date that is never promoted to an instant. Not an independent source. |

Fields:

- `case.anchor`: where the camera starts and returns to with Home. Not a record.
- `case.interval` and `case.clock`: reported local times with their zone. No UTC instant is recorded for anything in this case, and the build does not compute one.
- `records[].geometry.basis`: who placed the geometry and on what description. Shown in the inspector.
- `records[].reported_time.precision`: `approximate_window` (both bounds and a zone required), `calendar_date`, or `not_recorded` (no window allowed).

Validation (bounds, finite numbers, unique ids and orders, source references, text limits) is in `crates/oia-app/src/globe/mod.rs`; an invalid fixture is reported in the instrument as a fixture error and nothing is drawn.
