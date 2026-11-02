<#
.SYNOPSIS
Build, check, and launch the runnable OIA code that exists on main.

.DESCRIPTION
There is no root Cargo workspace or OIA application binary yet. The only
runnable entry point is the read-only inspect-text example of the standalone
crates/oia-documents package (DOC-02 slice of #17). Update this script when
the P0 workspace (#9) lands. Mirrors scripts/run.sh.

  build          Build the package and its inspection example.
  check          Run the package-local fmt, clippy, and test checks.
  run [ARGS...]  Run inspect-text: PATH [MAX_INPUT_BYTES] [MAX_PASSAGES].
                 Defaults to inspecting README.md when no PATH is given.

OIA_RUST_TOOLCHAIN overrides the pinned toolchain (1.99.0).

.EXAMPLE
.\scripts\run.ps1 run README.md 1048576 2
#>
param(
    [ValidateSet('build', 'check', 'run', 'help')]
    [string]$Command = 'run',
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Arguments = @()
)

# Exit codes are checked explicitly. 'Stop' would turn cargo's ordinary stderr
# progress into a terminating error when the caller redirects output (PS 5.1).
$Root = Split-Path -Parent $PSScriptRoot
$Manifest = Join-Path $Root 'crates\oia-documents\Cargo.toml'
$Toolchain = if ($env:OIA_RUST_TOOLCHAIN) { $env:OIA_RUST_TOOLCHAIN } else { '1.99.0' }

if ($Command -eq 'help') {
    Get-Help $PSCommandPath -Detailed
    exit 0
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

# The package lockfile has no third-party packages, so builds stay offline.
function Invoke-Package([string]$Subcommand, [string[]]$Rest) {
    & cargo @ToolchainArg $Subcommand --manifest-path $Manifest --locked --offline @Rest
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

if ($Arguments.Count -gt 0 -and $Arguments[0] -eq '--') {
    $Arguments = @($Arguments | Select-Object -Skip 1)
}

switch ($Command) {
    'build' {
        Invoke-Package 'build' @('--all-targets')
    }
    'check' {
        & cargo @ToolchainArg fmt --manifest-path $Manifest --all -- --check
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        Invoke-Package 'clippy' @('--workspace', '--all-targets', '--', '-D', 'warnings')
        Invoke-Package 'test' @('--workspace')
    }
    'run' {
        if ($Arguments.Count -eq 0) { $Arguments = @(Join-Path $Root 'README.md') }
        # Exit codes pass through: 0 complete/empty, 2 partial, 1 error.
        Invoke-Package 'run' (@('--quiet', '--example', 'inspect-text', '--') + $Arguments)
    }
}
exit 0
