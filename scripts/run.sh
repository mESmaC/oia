#!/usr/bin/env sh
# Build, check, and launch the OIA desktop workbench and CLI examples.
#
# Runs Cargo against the root OIA workspace (`Cargo.toml`) using the
# pinned Rust toolchain (1.99.0). `proofs` checks Lean and fixture synchronization;
# `check` requires both Rust checks and the complete Lean validation gate.
#
# Usage: scripts/run.sh [fetch|build|fmt|lint|test|proofs|check|run|inspect|search] [-- ARGS...]
#   fetch             Download the locked dependencies and the pinned CesiumJS build once
#                     (the only online step; see scripts/fetch-ui-vendor.sh).
#   build             Build the entire workspace (`--workspace --all-targets`).
#   fmt               Check formatting across the workspace.
#   lint              Run clippy across the workspace (`-D warnings`).
#   test              Run all workspace tests.
#   proofs            Check the pinned Lean package, axiom audit, and shared fixture.
#   check             Run proofs plus workspace fmt, clippy, and test (nothing skipped).
#   run               Launch the OIA desktop workbench (arguments retain the inspect alias).
#   inspect [ARGS...] Run inspect-text: PATH [MAX_INPUT_BYTES] [MAX_PASSAGES].
#                     Defaults to README.md when no PATH is given.
#   search ARGS...    Run `oia-documents` find-text: PATH QUERY [MAX_MATCHES] [MAX_INPUT_BYTES]
#                     [MAX_PASSAGES]. Zero matches cover only the file read,
#                     not absence from the source.
#
# Environment: OIA_RUST_TOOLCHAIN overrides the pinned toolchain (1.99.0).
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
MANIFEST="$ROOT/Cargo.toml"
PROOFS_DIR="$ROOT/proofs"
TOOLCHAIN=${OIA_RUST_TOOLCHAIN:-1.99.0}

proofs() {
    if [ ! -f "$PROOFS_DIR/lean-toolchain" ]; then
        echo "run.sh: required proofs/ package is missing" >&2
        exit 1
    fi
    if ! command -v lake >/dev/null 2>&1; then
        echo "run.sh: lake not found; install elan, then: elan toolchain install leanprover/lean4:v4.35.0 (see proofs/README.md)" >&2
        exit 1
    fi
    # Re-audit on every run to detect new modules outside the cached import graph.
    (cd "$PROOFS_DIR" && lake build && lake env lean Audit.lean &&
        lake env lean --run ExportFixtures.lean --check)
}
# Proof-only validation does not require Cargo.
if [ "${1:-}" = "proofs" ]; then
    proofs
    exit 0
fi
if [ "${1:-}" = "check" ] && ! command -v lake >/dev/null 2>&1; then
    echo "run.sh: lake is required for check; install elan, then: elan toolchain install leanprover/lean4:v4.35.0 (see proofs/README.md). Use fmt/lint/test for Rust-only checks." >&2
    exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "run.sh: cargo not found; install Rust $TOOLCHAIN with rustup" >&2
    exit 1
fi
if command -v rustup >/dev/null 2>&1; then
    if ! rustup run "$TOOLCHAIN" cargo --version >/dev/null 2>&1; then
        echo "run.sh: Rust toolchain $TOOLCHAIN is not installed; run: rustup toolchain install $TOOLCHAIN" >&2
        exit 1
    fi
    cargo() { command cargo "+$TOOLCHAIN" "$@"; }
else
    echo "run.sh: rustup not found; using $(command cargo --version) instead of pinned $TOOLCHAIN" >&2
fi

# Workspace commands run offline against the committed lockfile. `fetch` is the
# one online step; run it once (or after a lockfile change) to cache the
# locked dependencies, including the desktop ones.
ws() {
    subcommand=$1
    shift
    cargo "$subcommand" --manifest-path "$MANIFEST" --locked --offline "$@"
}

action=${1:-run}
[ $# -gt 0 ] && shift
[ "${1:-}" = "--" ] && shift

case $action in
    fetch)
        cargo fetch --manifest-path "$MANIFEST" --locked
        sh "$ROOT/scripts/fetch-ui-vendor.sh"
        ;;
    build)
        ws build --workspace --all-targets
        ;;
    fmt)
        cargo fmt --manifest-path "$MANIFEST" --all -- --check
        ;;
    lint)
        ws clippy --workspace --all-targets -- -D warnings
        ;;
    test)
        ws test --workspace
        ;;
    check)
        proofs
        cargo fmt --manifest-path "$MANIFEST" --all -- --check
        ws clippy --workspace --all-targets -- -D warnings
        ws test --workspace
        ;;
    run)
        if [ $# -eq 0 ]; then
            if [ ! -f "$ROOT/crates/oia-app/ui/vendor/cesium/VERSION" ]; then
                echo "run.sh: the CesiumJS build is not fetched; the Globe will report it as not present. Run: scripts/run.sh fetch" >&2
            fi
            ws run -p oia-app --bin oia
        else
            ws run --quiet -p oia-documents --example inspect-text -- "$@"
        fi
        ;;
    inspect)
        [ $# -eq 0 ] && set -- "$ROOT/README.md"
        # Exit codes pass through: 0 complete/empty, 2 partial, 1 error.
        ws run --quiet -p oia-documents --example inspect-text -- "$@"
        ;;
    search)
        # Exit codes pass through: 0 complete (including zero matches),
        # 2 partial, 1 error. find-text reports its own usage errors.
        ws run --quiet -p oia-documents --example find-text -- "$@"
        ;;
    -h|--help|help)
        sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
        ;;
    *)
        echo "run.sh: unknown command '$action'; use fetch, build, fmt, lint, test, proofs, check, run, inspect, or search" >&2
        exit 1
        ;;
esac
