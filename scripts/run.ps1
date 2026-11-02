<#
.SYNOPSIS
Build, check, and launch the OIA Rust workspace and CLI/examples.

.DESCRIPTION
Runs Cargo against the root 15-crate OIA workspace (`Cargo.toml`) using the
pinned Rust toolchain (1.99.0) and runs `lake build` in `proofs/` during `check`
when `lake` is available on PATH. Mirrors `scripts/run.sh`.

  build             Build the entire workspace (`--workspace --all-targets`).
  fmt               Check formatting across the workspace (`cargo fmt --all -- --check`).
  lint              Run clippy across the workspace (`--workspace --all-targets -- -D warnings`).
  test              Run all workspace tests (`cargo test --workspace`).
  check             Run workspace fmt, clippy, test, and `lake build` in `proofs/`.
  run [ARGS...]     Run `oia-documents` `inspect-text`: PATH [MAX_INPUT_BYTES] [MAX_PASSAGES].
                    Defaults to inspecting README.md when no PATH is given.
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
    [ValidateSet('build', 'fmt', 'lint', 'test', 'check', 'run', 'search', 'help')]
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

# The workspace lockfile has no third-party packages, so builds stay offline.
function Invoke-Workspace([string]$Subcommand, [string[]]$Rest) {
    & cargo @ToolchainArg $Subcommand --manifest-path $Manifest --locked --offline @Rest
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

if ($Arguments.Count -gt 0 -and $Arguments[0] -eq '--') {
    $Arguments = @($Arguments | Select-Object -Skip 1)
}

switch ($Command) {
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
        & cargo @ToolchainArg fmt --manifest-path $Manifest --all -- --check
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        Invoke-Workspace 'clippy' @('--workspace', '--all-targets', '--', '-D', 'warnings')
        Invoke-Workspace 'test' @('--workspace')
        if (Get-Command lake -ErrorAction SilentlyContinue) {
            Push-Location $ProofsDir
            try {
                & lake build
                if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            } finally {
                Pop-Location
            }
        }
    }
    'run' {
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
