# Third-Party Notices

Open Intelligence Agency (OIA) original source code is dual-licensed under
`MIT OR Apache-2.0` (see [`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE)).

**Important exclusion:** OIA's `MIT OR Apache-2.0` source-code license does **not** cover
third-party datasets, reference fixtures, upstream feeds, public records, media assets,
or models. Those materials remain governed by their respective upstream terms and
attribution requirements recorded below and in each fixture or source manifest.

---

## 1. God's Eye View (selective design and algorithm reuse)

- **Upstream repository:** https://github.com/bilawalsidhu/gods-eye-view
- **Inspected revision:** `0d1a68160e532ba65dc34b48ea205de6de4028fc` (2026-11-01)
- **Scope in repository:** Selective inspection and design/algorithm reference documented in [`docs/prior-art.md`](docs/prior-art.md) and [`docs/design/workspace-ui-ux.md`](docs/design/workspace-ui-ux.md). Module-level ports are recorded in `docs/prior-art.md` as each implementation package lands.

### License notice (God's Eye View source code)

```text
MIT License

Copyright (c) 2026 Bilawal Sidhu

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

---

## 2. NASA / JPL Solar System Dynamics — Horizons Reference Fixtures (`fixtures/sky/`)

- **Upstream service:** NASA/JPL Solar System Dynamics Group, Horizons API (`https://ssd.jpl.nasa.gov/api/horizons.api`)
- **Terms consulted:** https://www.jpl.nasa.gov/jpl-image-use-policy/ and https://ssd.jpl.nasa.gov/horizons/
- **Scope in repository:** Retained raw JSON API responses in [`fixtures/sky/horizons/`](fixtures/sky/README.md) used as deterministic reference data for `#25` (`A04`, `A05`, `SPACE-04`, `SPACE-05`).
- **Attribution & conditions:** Courtesy NASA/JPL-Caltech (Solar System Dynamics Group, Jet Propulsion Laboratory, California Institute of Technology). These reference responses are third-party ephemeris outputs, not OIA source code, and are not licensed under `MIT OR Apache-2.0`. NASA/JPL endorsement is not implied.

---

## 3. CesiumJS (Globe instrument renderer; fetched, not committed)

- **Upstream:** https://github.com/CesiumGS/cesium — npm package `cesium` **1.147.0**, tarball SHA-512 `818e5317…f24893` (full value in `scripts/fetch-ui-vendor.sh`).
- **License:** Apache License 2.0, Copyright 2011-2026 CesiumJS Contributors. The package's `LICENSE.md` and its `ThirdParty.json` (bundled libraries under Apache-2.0, BSD-3-Clause, ISC and MIT) are copied beside the build into `crates/oia-app/ui/vendor/cesium/` by the fetch step.
- **Scope in repository:** Nothing from Cesium is committed. `scripts/fetch-ui-vendor.{sh,ps1}` download the pinned tarball during the one-time `fetch` step, verify its hash, and keep only the minified browser build (`Cesium.js`), `Assets/`, `Workers/` and `ThirdParty/`. The vendor directory is gitignored and is embedded into the desktop binary at build time.
- **Modification:** one load-time patch is applied by the fetch scripts and recorded in [`docs/implementation/globe-instrument.md`](docs/implementation/globe-instrument.md): the bundled Knockout library's global-object probe `var t=this||(0,eval)("this")` becomes `var t=globalThis`, because the workbench CSP forbids `eval`. OIA uses no Cesium widget, so no behaviour that depends on Knockout is exercised. The Apache-2.0 notice requirement for modified files is met by this record.
- **Not used:** Cesium Ion (no token, no Ion assets, no Bing or Cesium World Terrain), the widget stylesheet and widget images, and the ESM/CJS builds.

---

## 4. NASA Global Imagery Browse Services (GIBS) — live imagery in the Globe instrument

- **Upstream service:** NASA GIBS WMTS, `https://gibs.earthdata.nasa.gov/wmts/epsg3857/best/` (ESDIS). Layers registered in `crates/oia-app/src/globe/imagery.rs`: `BlueMarble_ShadedRelief_Bathymetry`, `VIIRS_SNPP_CorrectedReflectance_TrueColor`. Capabilities checked 2026-11-03.
- **Terms consulted:** NASA Earthdata data use policy, https://www.earthdata.nasa.gov/engage/open-data-services-software-policies/data-use-policy (U.S. Government work, public domain; keyless; no bulk use). Both registered layers are NASA data under that policy. The GIBS `Coastlines_15m` reference layer (OpenStreetMap-derived, © OpenStreetMap contributors, ODbL 1.0) was evaluated for an overlay and withdrawn on 2026-11-03 before merge; it is not registered, requested or attributed in this build.
- **Scope in repository:** No tiles are stored. The desktop page requests tiles at runtime from this origin only, bounded to the visible view, and shows the layer, date and tile counts. Tiles are display imagery for orientation; they are never presented as observations of a case.
- **Acknowledgement (shown in the instrument):** "We acknowledge the use of imagery provided by services from NASA's Global Imagery Browse Services (GIBS), part of NASA's Earth Science Data and Information System (ESDIS)." Blue Marble Next Generation: Reto Stöckli, NASA Earth Observatory. NASA endorsement is not implied.
