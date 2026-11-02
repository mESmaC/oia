# ALPR / Flock camera corridor fixture

Synthetic OpenStreetMap Overpass-compatible node fixture for validating OIA's mapped ALPR / Flock camera directory kernel ([`crates/oia-alpr-map`](../../../crates/oia-alpr-map/README.md)) and interactive map draft ([`docs/design/mockups/flock-camera-map.html`](../../../docs/design/mockups/flock-camera-map.html)).

- **Origin class:** Synthetic test fixture (`origin: "synthetic-fixture"`), modeled on OpenStreetMap `man_made=surveillance` + `surveillance:type=ALPR` tagging conventions in central Austin, TX (`30.255..30.285 N`, `-97.765..-97.725 E`).
- **Not live or retained field evidence:** Node IDs, coordinates, operator strings, `check_date` / `survey:date` values, and `osm_timestamp` values are deterministic fixture inputs for testing normalization, manufacturer classification (`FlockSafety`, `OtherNamed`, `Unspecified`), semicolon tag parsing (`camera;ALPR`), bearing edge cases (blank/whitespace, cardinal words `"N"`, `"360"` wrap to `0.0`), non-ALPR exclusion, and saturation/coverage reporting.
- **License:** Repository fixture authored for OIA validation; real OpenStreetMap extracts in production remain subject to ODbL 1.0 (`© OpenStreetMap contributors`).
