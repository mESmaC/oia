# Downloads the pinned CesiumJS build used by the Globe instrument into
# crates\oia-app\ui\vendor\cesium (gitignored). Part of `scripts\run.ps1 fetch`.
#
# The npm tarball is verified against its published SHA-512 before anything is
# extracted. Only the minified browser build, its Assets, Workers and ThirdParty
# directories, and the licence notices are kept; the ESM/CJS builds and widget
# stylesheets are not used by OIA. Cesium is Apache-2.0; see THIRD_PARTY_NOTICES.md.
$ErrorActionPreference = 'Stop'

$CesiumVersion = '1.147.0'
$CesiumSha512 = '818e53174ad2c8832d9d22051d4ba21e7ac2ba174a6875332d615c8f1cadb1d85e29f2ad61839689c216332a468f061f9e626b80f80d17b1cd8cef522bf24893'
$CesiumUrl = "https://registry.npmjs.org/cesium/-/cesium-$CesiumVersion.tgz"

$Root = Split-Path -Parent $PSScriptRoot
$Dest = Join-Path $Root 'crates\oia-app\ui\vendor\cesium'
$VersionFile = Join-Path $Dest 'VERSION'

if ((Test-Path $VersionFile) -and ((Get-Content $VersionFile -Raw).Trim() -eq $CesiumVersion)) {
    Write-Host "fetch-ui-vendor: CesiumJS $CesiumVersion already present in crates\oia-app\ui\vendor\cesium"
    exit 0
}

$Tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('oia-cesium-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $Tmp | Out-Null
try {
    $Tgz = Join-Path $Tmp 'cesium.tgz'
    Write-Host "fetch-ui-vendor: downloading CesiumJS $CesiumVersion"
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -Uri $CesiumUrl -OutFile $Tgz -UseBasicParsing

    $Actual = (Get-FileHash -Algorithm SHA512 $Tgz).Hash.ToLowerInvariant()
    if ($Actual -ne $CesiumSha512) {
        throw "fetch-ui-vendor: SHA-512 mismatch for cesium-$CesiumVersion.tgz; nothing installed"
    }

    & tar -xzf $Tgz -C $Tmp `
        package/LICENSE.md package/ThirdParty.json `
        package/Build/Cesium/Cesium.js package/Build/Cesium/Assets `
        package/Build/Cesium/Workers package/Build/Cesium/ThirdParty
    if ($LASTEXITCODE -ne 0) { throw "fetch-ui-vendor: tar failed with exit code $LASTEXITCODE" }

    # One recorded patch (docs/implementation/globe-instrument.md): the bundled Knockout
    # library resolves the global object with `(0,eval)("this")` while the bundle loads,
    # which the workbench CSP refuses (no 'unsafe-eval'). OIA uses no Cesium widget, so the
    # global is taken from `globalThis` instead. The replacement must match exactly once;
    # a Cesium upgrade that changes this line fails here, so the patch is re-examined.
    $Bundle = Join-Path $Tmp 'package\Build\Cesium\Cesium.js'
    $PatchFrom = 'var t=this||(0,eval)("this")'
    $PatchTo = 'var t=globalThis'
    $Utf8 = New-Object System.Text.UTF8Encoding($false)
    $Text = [System.IO.File]::ReadAllText($Bundle, $Utf8)
    $First = $Text.IndexOf($PatchFrom, [System.StringComparison]::Ordinal)
    if ($First -lt 0 -or $Text.IndexOf($PatchFrom, $First + 1, [System.StringComparison]::Ordinal) -ge 0) {
        throw 'fetch-ui-vendor: expected exactly one Knockout global probe in Cesium.js; refusing to patch'
    }
    [System.IO.File]::WriteAllText($Bundle, $Text.Replace($PatchFrom, $PatchTo), $Utf8)

    New-Item -ItemType Directory -Force (Split-Path -Parent $Dest) | Out-Null
    if (Test-Path $Dest) { Remove-Item -Recurse -Force $Dest }
    Move-Item (Join-Path $Tmp 'package\Build\Cesium') $Dest
    Copy-Item (Join-Path $Tmp 'package\LICENSE.md') (Join-Path $Dest 'LICENSE.md')
    Copy-Item (Join-Path $Tmp 'package\ThirdParty.json') (Join-Path $Dest 'ThirdParty.json')
    [System.IO.File]::WriteAllText($VersionFile, $CesiumVersion)
    Write-Host "fetch-ui-vendor: installed CesiumJS $CesiumVersion (verified SHA-512)"
} finally {
    if (Test-Path $Tmp) { Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue }
}
