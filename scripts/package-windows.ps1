<#
.SYNOPSIS
Build an unsigned, local Windows development ZIP from a clean Git revision.
.DESCRIPTION
Run scripts/run.ps1 fetch first. No network, installer, signing or publication.
Use -WithoutGlobe explicitly to build without the optional CesiumJS renderer.
#>
param(
    [ValidateSet('Release', 'Debug')]
    [string]$Configuration = 'Release',
    [string]$OutputPath,
    [switch]$WithoutGlobe
)
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$Utf8 = New-Object System.Text.UTF8Encoding($false)
function Write-Utf8([string]$Path, [string]$Text) {
    [IO.File]::WriteAllText($Path, $Text, $Utf8)
}
function Git-Output([string[]]$GitArgs) {
    $Result = & git -C $Root @GitArgs
    if ($LASTEXITCODE -ne 0) { throw 'package-windows: git failed' }
    return $Result
}
function Assert-Clean {
    if (Git-Output @('status', '--porcelain', '--untracked-files=all')) {
        throw 'package-windows: commit or move pending changes before packaging; nothing was changed'
    }
}
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw 'package-windows: Windows is required'
}
Assert-Clean
$Revision = Git-Output @('rev-parse', 'HEAD')
$Target = 'x86_64-pc-windows-msvc'
$Profile = $Configuration.ToLowerInvariant()
if (-not $OutputPath) {
    $OutputPath = Join-Path $Root "target/packages/oia-dev-$($Revision.Substring(0, 12))-$Target-$Profile.zip"
}
$OutputPath = [IO.Path]::GetFullPath($OutputPath)
if ([IO.Path]::GetExtension($OutputPath) -ne '.zip') { throw 'package-windows: OutputPath must end in .zip' }
if (Test-Path -LiteralPath $OutputPath) { throw 'package-windows: output already exists; choose a new path' }

# Build a Git snapshot, so local edits during compilation cannot change the
# recorded source revision. Ignored files are excluded unless listed below.
$Work = Join-Path $Root ('target/package-windows/' + [guid]::NewGuid().ToString('N'))
$Source = Join-Path $Work 'source'
$Payload = Join-Path $Work 'payload'
New-Item -ItemType Directory -Path $Source, $Payload -Force | Out-Null
# Hold the lock through copying the executable. Cargo's own lock ends at build
# completion and would otherwise let a second packager replace our binary.
try {
    $BuildLock = [IO.File]::Open((Join-Path $Root 'target/package-windows/build.lock'), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
} catch {
    throw 'package-windows: another package build holds the build lock; retry after it finishes'
}
try {
Add-Type -AssemblyName System.IO.Compression.FileSystem
$SourceZip = Join-Path $Work 'source.zip'
& git -C $Root archive --format=zip "--output=$SourceZip" $Revision
if ($LASTEXITCODE -ne 0) { throw 'package-windows: source snapshot failed' }
[IO.Compression.ZipFile]::ExtractToDirectory($SourceZip, $Source)
$VendorInventory = @()
$Vendor = Join-Path $Root 'crates/oia-app/ui/vendor/cesium'
if (-not $WithoutGlobe) {
    foreach ($Required in @('VERSION', 'LICENSE.md', 'ThirdParty.json', 'Cesium.js', 'Assets', 'Workers', 'ThirdParty')) {
        if (-not (Test-Path -LiteralPath (Join-Path $Vendor $Required))) {
            throw 'package-windows: CesiumJS is incomplete; run scripts/run.ps1 fetch or explicitly use -WithoutGlobe'
        }
    }
    $FetchScript = [IO.File]::ReadAllText((Join-Path $Source 'scripts/fetch-ui-vendor.ps1'))
    $Pin = [regex]::Match($FetchScript, '(?m)^\$CesiumVersion = ''([^'']+)''').Groups[1].Value
    if (-not $Pin -or ([IO.File]::ReadAllText((Join-Path $Vendor 'VERSION'))).Trim() -ne $Pin) {
        throw 'package-windows: vendor version does not match the fetch script pin'
    }
    $VendorCopy = Join-Path $Source 'crates/oia-app/ui/vendor/cesium'
    New-Item -ItemType Directory -Path (Split-Path -Parent $VendorCopy) -Force | Out-Null
    Copy-Item -LiteralPath $Vendor -Destination $VendorCopy -Recurse
    $VendorInventory = @(Get-ChildItem -LiteralPath $VendorCopy -File -Recurse | Sort-Object FullName | ForEach-Object {
        [ordered]@{ path = $_.FullName.Substring($VendorCopy.Length + 1).Replace('\', '/'); sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
    })
    Copy-Item -LiteralPath (Join-Path $VendorCopy 'LICENSE.md') -Destination (Join-Path $Payload 'Cesium-LICENSE.md')
    Copy-Item -LiteralPath (Join-Path $VendorCopy 'ThirdParty.json') -Destination (Join-Path $Payload 'Cesium-ThirdParty.json')
}
$Toolchain = ([IO.File]::ReadAllText((Join-Path $Source 'rust-toolchain.toml')) | Select-String 'channel\s*=\s*"([^"]+)"').Matches.Groups[1].Value
if (-not $Toolchain) { throw 'package-windows: missing pinned Rust toolchain' }
$Compiler = & rustup run $Toolchain rustc --version --verbose
if ($LASTEXITCODE -ne 0) { throw 'package-windows: pinned Rust compiler unavailable' }
$BuildArgs = @('build', '--manifest-path', (Join-Path $Source 'Cargo.toml'), '--locked', '--offline', '-p', 'oia-app', '--bin', 'oia', '--target', $Target, '--target-dir', (Join-Path $Root 'target/package-windows/build'))
if ($Configuration -eq 'Release') { $BuildArgs += '--release' }
& rustup run $Toolchain cargo @BuildArgs
if ($LASTEXITCODE -ne 0) { throw 'package-windows: desktop build failed; no archive published' }
$Binary = Join-Path $Root "target/package-windows/build/$Target/$Profile/oia.exe"
Copy-Item -LiteralPath $Binary -Destination (Join-Path $Payload 'oia.exe')
foreach ($Name in @('LICENSE-MIT', 'LICENSE-APACHE', 'THIRD_PARTY_NOTICES.md', 'README.md', 'Cargo.lock')) {
    Copy-Item -LiteralPath (Join-Path $Source $Name) -Destination $Payload
}
Write-Utf8 (Join-Path $Payload 'PACKAGE.txt') @"
OIA local development build (unsigned)
Source: https://github.com/mESmaC/oia/commit/$Revision
Run oia.exe on Windows x64 with Microsoft WebView2 Runtime installed.
Build profile: $Profile. Optional Globe renderer included: $(-not $WithoutGlobe).
Documents, images and witness accounts are session-only. See README.md for limits.
Globe imagery requires network access and source-specific terms in THIRD_PARTY_NOTICES.md.
No installer, automatic updates, code signing, or release acceptance is established.
This archive includes repository notices; a complete transitive dependency license
audit remains required before public distribution. No source datasets are included.
Build metadata records inputs, not authenticity, bit reproducibility or test results.
"@
$Files = @(Get-ChildItem -LiteralPath $Payload -File | Sort-Object Name | ForEach-Object {
    [ordered]@{ path = $_.Name; bytes = $_.Length; sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
})
$Metadata = [ordered]@{
    schema_version = 1; artifact_class = 'unsigned-local-development'; revision = $Revision
    built_at_utc = [DateTime]::UtcNow.ToString('o'); target = $Target; profile = $Profile
    rust_toolchain = $Toolchain; rustc = @($Compiler); os = [Environment]::OSVersion.VersionString
    cargo_arguments = $BuildArgs; globe_renderer_included = [bool](-not $WithoutGlobe)
    vendor_files = $VendorInventory; files = $Files
    validation = 'Packaging does not run or attest release gates. Record separate local check evidence.'
}
Write-Utf8 (Join-Path $Payload 'build-manifest.json') ($Metadata | ConvertTo-Json -Depth 8)
$Archive = Join-Path $Work 'package.zip'
[IO.Compression.ZipFile]::CreateFromDirectory($Payload, $Archive)
# File.Move never overwrites, including when another process wins the race.
New-Item -ItemType Directory -Path (Split-Path -Parent $OutputPath) -Force | Out-Null
[IO.File]::Move($Archive, $OutputPath)
Write-Host "package-windows: $OutputPath"
Write-Host "SHA256 $((Get-FileHash -LiteralPath $OutputPath -Algorithm SHA256).Hash.ToLowerInvariant())"
# Keep the task-owned snapshot and manifest for inspection. All intermediates are
# under ignored target/; no recursive cleanup of caller-supplied paths is needed.
} finally {
    $BuildLock.Dispose()
}
