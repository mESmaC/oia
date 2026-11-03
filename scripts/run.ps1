<#
.SYNOPSIS
Build, check, and launch the OIA desktop workbench and CLI examples.

.DESCRIPTION
Runs Cargo against the root OIA workspace (`Cargo.toml`) using the
pinned Rust toolchain (1.99.0). `proofs` checks Lean and fixture synchronization;
`check` requires both the Rust checks and the complete Lean validation gate.
Mirrors `scripts/run.sh`.

  fetch             Download the locked dependencies and the pinned CesiumJS build once
                    (the only online step; see scripts\fetch-ui-vendor.ps1).
  build             Build the entire workspace (`--workspace --all-targets`).
  fmt               Check formatting across the workspace (`cargo fmt --all -- --check`).
  lint              Run clippy across the workspace (`--workspace --all-targets -- -D warnings`).
  test              Run all workspace tests (`cargo test --workspace`).
  proofs            Check the pinned Lean package, axiom audit, and shared fixture.
  check             Run proofs plus workspace fmt, clippy, and test (nothing skipped).
  run               Launch the OIA desktop workbench.
  run PATH [...]    Compatibility alias for inspect.
  inspect [ARGS...] Run `inspect-text`: PATH [MAX_INPUT_BYTES] [MAX_PASSAGES].
                    Defaults to README.md when no PATH is given.
  search ARGS...    Run `oia-documents` `find-text`: PATH QUERY [MAX_MATCHES] [MAX_INPUT_BYTES]
                    [MAX_PASSAGES]. Zero matches cover only the file read,
                    not absence from the source.

OIA_RUST_TOOLCHAIN overrides the pinned toolchain (1.99.0).

.EXAMPLE
.\scripts\run.ps1 check

.EXAMPLE
.\scripts\run.ps1 run README.md 1048576 2

.EXAMPLE
.\scripts\run.ps1 search fixtures\documents\a03\originals\report-v1.txt "two lights"
#>
param(
    [ValidateSet('fetch', 'build', 'fmt', 'lint', 'test', 'proofs', 'check', 'run', 'inspect', 'search', 'help')]
    [string]$Command = 'run',
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Arguments = @()
)

# Exit codes are checked explicitly. 'Stop' would turn cargo's ordinary stderr
# progress into a terminating error when the caller redirects output (PS 5.1).
$Root = Split-Path -Parent $PSScriptRoot
$Manifest = Join-Path $Root 'Cargo.toml'
$ProofsDir = Join-Path $Root 'proofs'
$Toolchain = if ($env:OIA_RUST_TOOLCHAIN) { $env:OIA_RUST_TOOLCHAIN } else { '1.99.0' }

if ($Command -eq 'help') {
    Get-Help $PSCommandPath -Detailed
    exit 0
}
function Invoke-Proofs {
    if (-not (Test-Path (Join-Path $ProofsDir 'lean-toolchain'))) {
        [Console]::Error.WriteLine('run.ps1: required proofs/ package is missing')
        exit 1
    }
    if (-not (Get-Command lake -ErrorAction SilentlyContinue)) {
        [Console]::Error.WriteLine('run.ps1: lake not found; install elan, then: elan toolchain install leanprover/lean4:v4.35.0 (see proofs/README.md)')
        exit 1
    }
    Push-Location $ProofsDir
    try {
        & lake build
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        # Re-run the audit even if Lake's import graph is cached, so a newly
        # added but unimported source file cannot evade validation.
        & lake env lean Audit.lean
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        & lake env lean --run ExportFixtures.lean --check
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    } finally {
        Pop-Location
    }
}
# Proof-only validation does not require Cargo. Full checks fail before Cargo if
# Lean or the proof package is unavailable.
if ($Command -eq 'proofs') { Invoke-Proofs; exit 0 }
if ($Command -eq 'check' -and -not (Get-Command lake -ErrorAction SilentlyContinue)) {
    [Console]::Error.WriteLine('run.ps1: lake is required for check; install elan, then: elan toolchain install leanprover/lean4:v4.35.0 (see proofs/README.md). Use fmt/lint/test for Rust-only checks.')
    exit 1
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    [Console]::Error.WriteLine("run.ps1: cargo not found; install Rust $Toolchain with rustup")
    exit 1
}
$ToolchainArg = @()
if (Get-Command rustup -ErrorAction SilentlyContinue) {
    & rustup run $Toolchain cargo --version *> $null
    if ($LASTEXITCODE -ne 0) {
        [Console]::Error.WriteLine("run.ps1: Rust toolchain $Toolchain is not installed; run: rustup toolchain install $Toolchain")
        exit 1
    }
    $ToolchainArg = @("+$Toolchain")
} else {
    [Console]::Error.WriteLine("run.ps1: rustup not found; using $(cargo --version) instead of pinned $Toolchain")
}

# Workspace commands run offline against the committed lockfile. `fetch` is the
# one online step; run it once (or after a lockfile change) to cache the
# locked dependencies, including the desktop ones.
function Invoke-Workspace([string]$Subcommand, [string[]]$Rest) {
    & cargo @ToolchainArg $Subcommand --manifest-path $Manifest --locked --offline @Rest
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

if ($Arguments.Count -gt 0 -and $Arguments[0] -eq '--') {
    $Arguments = @($Arguments | Select-Object -Skip 1)
}

switch ($Command) {
    'fetch' {
        & cargo @ToolchainArg fetch --manifest-path $Manifest --locked
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'fetch-ui-vendor.ps1')
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
    'build' {
        Invoke-Workspace 'build' @('--workspace', '--all-targets')
    }
    'fmt' {
        & cargo @ToolchainArg fmt --manifest-path $Manifest --all -- --check
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
    'lint' {
        Invoke-Workspace 'clippy' @('--workspace', '--all-targets', '--', '-D', 'warnings')
    }
    'test' {
        Invoke-Workspace 'test' @('--workspace')
    }
    'check' {
        Invoke-Proofs
        & cargo @ToolchainArg fmt --manifest-path $Manifest --all -- --check
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        Invoke-Workspace 'clippy' @('--workspace', '--all-targets', '--', '-D', 'warnings')
        Invoke-Workspace 'test' @('--workspace')
    }
    'run' {
        if ($Arguments.Count -eq 0) {
            if (-not (Test-Path (Join-Path $Root 'crates\oia-app\ui\vendor\cesium\VERSION'))) {
                [Console]::Error.WriteLine('run.ps1: the CesiumJS build is not fetched; the Globe will report it as not present. Run: .\scripts\run.ps1 fetch')
            }
            Invoke-Workspace 'run' @('-p', 'oia-app', '--bin', 'oia')
        } else {
            Invoke-Workspace 'run' (@('--quiet', '-p', 'oia-documents', '--example', 'inspect-text', '--') + $Arguments)
        }
    }
    'inspect' {
        if ($Arguments.Count -eq 0) { $Arguments = @(Join-Path $Root 'README.md') }
        # Exit codes pass through: 0 complete/empty, 2 partial, 1 error.
        Invoke-Workspace 'run' (@('--quiet', '-p', 'oia-documents', '--example', 'inspect-text', '--') + $Arguments)
    }
    'search' {
        # Exit codes pass through: 0 complete (including zero matches),
        # 2 partial, 1 error. find-text reports its own usage errors.
        Invoke-Workspace 'run' (@('--quiet', '-p', 'oia-documents', '--example', 'find-text', '--') + $Arguments)
    }
}
exit 0
