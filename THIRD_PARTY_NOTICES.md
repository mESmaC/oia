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
