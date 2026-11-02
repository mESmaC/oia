# Third-Party Notices

This file records required notices and attributions for third-party source code
selectively ported or adapted into Open Intelligence Agency (OIA).

 Upstream data sources, public records, media feeds, ephemerides, and models are
governed by their own terms and are tracked per-source in `oia-ingest` registrations
and `oia-export` bundle manifests rather than licensed under the repository's code
license.

---

## God's Eye View (selective algorithm and contract ports)

- **Upstream repository:** https://github.com/bilawalsidhu/gods-eye-view
- **Inspected revision:** `0d1a68160e532ba65dc34b48ea205de6de4028fc` (2026-11-01)
- **Ported / adapted scope in P0:**
  - Bounded request body reading with hard byte caps, overflow draining, and cancellation (`server/providers/common/request.js`, `request.test.mjs`) -> adapted in `crates/oia-ingest`.
  - Fail-safe rate-limit configuration resolution and bounded per-key/global sliding-window admission (`server/providers/common/rate-limit.js`, `src/sources/rateLimit.js`) -> adapted in `crates/oia-ingest`.
  - Per-product historical time-window frame selection with maximum gap enforcement and generation-guarded transitions (`src/layers/weather/clock.js`, `clock.test.mjs`) -> adapted in `crates/oia-time-geo`, `crates/oia-ingest`, and formally specified in `proofs/OiaProofs/FrameSelection.lean`.
  - Datum separation between WGS84 ellipsoidal height, orthometric MSL height, and barometric altitude (`src/data/geoid.js`) -> adapted in `crates/oia-time-geo`.
  - Camera frame provenance classification distinguishing upstream imagery, still fallback, Street View, and synthetic substitutes (`src/layers/cctv/source.js`, `sourcePolicy.js`) -> adapted in `crates/oia-ingest`.

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
