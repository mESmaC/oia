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
- **Scope in repository:** No tiles are stored. The desktop page requests basemap tiles at runtime from this origin, bounded to the visible view, and shows the layer, date and tile counts. The detail tiers in sections 5 and 6 come from their own registered origins. Tiles are display imagery for orientation; they are never presented as observations of a case.
- **Acknowledgement (shown in the instrument):** "We acknowledge the use of imagery provided by services from NASA's Global Imagery Browse Services (GIBS), part of NASA's Earth Science Data and Information System (ESDIS)." Blue Marble Next Generation: Reto Stöckli, NASA Earth Observatory. NASA endorsement is not implied.

---

## 5. EOX Sentinel-2 cloudless (EOxCloudless) — Globe detail imagery

- **Upstream service:** EOX WMTS, `https://tiles.maps.eox.at/wmts/1.0.0/`, layer `s2cloudless-2025_3857` (GoogleMapsCompatible), registered in `crates/oia-app/src/globe/imagery.rs` as the global detail tier. Capabilities checked 2026-11-03.
- **Licence:** Creative Commons Attribution-NonCommercial-ShareAlike 4.0 International for non-commercial use, as stated in the layer's capabilities abstract and the EOX licence summary (https://cloudless.eox.at/documentation/license). Commercial use requires a licence from EOX. The licence covers the imagery, not OIA's code.
- **Attribution (shown in the instrument whenever the tier is drawn):** "EOxCloudless https://cloudless.eox.at by EOX IT Services GmbH (Contains modified Copernicus Sentinel data 2025)".
- **Scope in repository:** No tiles are stored. The page requests them at runtime for the visible view only, from imagery level 7 or 8 to 14 (the 10 m source resolution), with at most six requests in flight. Turning detail off in the Imagery tab stops all requests to this origin.

---

## 6. USGS The National Map orthoimagery — Globe detail imagery (United States)

- **Upstream service:** USGS The National Map, `https://basemap.nationalmap.gov/arcgis/rest/services/USGSImageryOnly/MapServer` (tile cache to 1:9,028, level 16), registered in `crates/oia-app/src/globe/imagery.rs` as the United States detail tier, limited to the conterminous United States rectangle. Service description checked 2026-11-03 ("Data refreshed June, 2024").
- **Licence:** Public domain (U.S. Government work; USDA National Agriculture Imagery Program and USGS), per the USGS copyrights and credits policy (https://www.usgs.gov/information-policies-and-instructions/copyrights-and-credits). The service states that its 10 m Alaska SPOT imagery is licence-restricted; Alaska lies outside the registered rectangle and is not requested.
- **Attribution (shown in the instrument whenever the tier is drawn):** "USDA, USGS The National Map: Orthoimagery". USGS and USDA endorsement is not implied.
- **Scope in repository:** No tiles are stored. The page requests them at runtime for the visible view only, from imagery level 13 to 16, with at most six requests in flight. Turning detail off stops all requests to this origin.


## 7. USGS NAIP dynamic natural-colour imagery — closer Globe detail

- **Service:** https://imagery.nationalmap.gov/arcgis/rest/services/USGSNAIPImagery/ImageServer (description and bounded export checked 2026-11-03).
- **Licence and attribution:** Public domain USDA/USGS work; https://www.usgs.gov/information-policies-and-instructions/copyrights-and-credits. Shown as “USDA NAIP / USGS The National Map: NAIP Imagery”. No endorsement implied.
- **Use:** Visible-view 256 × 256 transparent PNG exports, bands 0/1/2, nearest-neighbour sampling, levels 16–18, six requests in flight, continental-US bounds. No bulk download or retained tile artifacts. These are orthorectified aerial mosaics, primarily 0.6 m with variable flight dates/resolution, not raw satellite data. Metadata grid spacing (0.3 m) is not asserted as native resolution.
