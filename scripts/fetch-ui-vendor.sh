#!/bin/sh
# Downloads the pinned CesiumJS build used by the Globe instrument into
# crates/oia-app/ui/vendor/cesium (gitignored). Part of `scripts/run.sh fetch`.
#
# The npm tarball is verified against its published SHA-512 before anything is
# extracted. Only the minified browser build, its Assets, Workers and ThirdParty
# directories, and the licence notices are kept; the ESM/CJS builds and widget
# stylesheets are not used by OIA. Cesium is Apache-2.0; see THIRD_PARTY_NOTICES.md.
set -eu

CESIUM_VERSION="1.147.0"
CESIUM_SHA512="818e53174ad2c8832d9d22051d4ba21e7ac2ba174a6875332d615c8f1cadb1d85e29f2ad61839689c216332a468f061f9e626b80f80d17b1cd8cef522bf24893"
CESIUM_URL="https://registry.npmjs.org/cesium/-/cesium-${CESIUM_VERSION}.tgz"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
DEST="$ROOT/crates/oia-app/ui/vendor/cesium"

if [ -f "$DEST/VERSION" ] && [ "$(cat "$DEST/VERSION")" = "$CESIUM_VERSION" ]; then
    echo "fetch-ui-vendor: CesiumJS $CESIUM_VERSION already present in crates/oia-app/ui/vendor/cesium"
    exit 0
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT INT TERM

echo "fetch-ui-vendor: downloading CesiumJS $CESIUM_VERSION"
curl -fsSL --max-time 600 -o "$TMP/cesium.tgz" "$CESIUM_URL"

if command -v sha512sum >/dev/null 2>&1; then
    ACTUAL=$(sha512sum "$TMP/cesium.tgz" | cut -d' ' -f1)
else
    ACTUAL=$(shasum -a 512 "$TMP/cesium.tgz" | cut -d' ' -f1)
fi
if [ "$ACTUAL" != "$CESIUM_SHA512" ]; then
    echo "fetch-ui-vendor: SHA-512 mismatch for cesium-$CESIUM_VERSION.tgz; nothing installed" >&2
    exit 1
fi

tar -xzf "$TMP/cesium.tgz" -C "$TMP" \
    package/LICENSE.md package/ThirdParty.json \
    package/Build/Cesium/Cesium.js package/Build/Cesium/Assets \
    package/Build/Cesium/Workers package/Build/Cesium/ThirdParty

# One recorded patch (docs/implementation/globe-instrument.md): the bundled Knockout
# library resolves the global object with `(0,eval)("this")` while the bundle loads,
# which the workbench CSP refuses (no 'unsafe-eval'). OIA uses no Cesium widget, so the
# global is taken from `globalThis` instead. The replacement must match exactly once;
# a Cesium upgrade that changes this line fails here, so the patch is re-examined.
BUNDLE="$TMP/package/Build/Cesium/Cesium.js"
PATCH_FROM='var t=this||(0,eval)("this")'
PATCH_TO='var t=globalThis'
if [ "$(grep -o -F "$PATCH_FROM" "$BUNDLE" | wc -l | tr -d ' ')" != "1" ]; then
    echo "fetch-ui-vendor: expected exactly one Knockout global probe in Cesium.js; refusing to patch" >&2
    exit 1
fi
sed -i.bak "s/var t=this||(0,eval)(\"this\")/$PATCH_TO/" "$BUNDLE" && rm -f "$BUNDLE.bak"
if grep -q -F "$PATCH_FROM" "$BUNDLE"; then
    echo "fetch-ui-vendor: patch did not apply" >&2
    exit 1
fi

mkdir -p "$(dirname "$DEST")"
rm -rf "$DEST"
mv "$TMP/package/Build/Cesium" "$DEST"
cp "$TMP/package/LICENSE.md" "$DEST/LICENSE.md"
cp "$TMP/package/ThirdParty.json" "$DEST/ThirdParty.json"
printf '%s' "$CESIUM_VERSION" > "$DEST/VERSION"
echo "fetch-ui-vendor: installed CesiumJS $CESIUM_VERSION (verified SHA-512)"
