#!/usr/bin/env sh
# Build, check, and launch the OIA Rust workspace and CLI/examples.
#
# Runs Cargo against the root 15-crate OIA workspace (`Cargo.toml`) using the
# pinned Rust toolchain (1.99.0), and runs `lake build` in `proofs/` during
# `check` when the `proofs/` directory exists and `lake` is available on PATH.
#
# Usage: scripts/run.sh [build|fmt|lint|test|check|run|search] [-- ARGS...]
#   build             Build the entire workspace (`--workspace --all-targets`).
#   fmt               Check formatting across the workspace.
#   lint              Run clippy across the workspace (`-D warnings`).
#   test              Run all workspace tests.
#   check             Run workspace fmt, clippy, test, and (when `proofs/` exists) `lake build`.
#   run [ARGS...]     Run `oia-documents` inspect-text: PATH [MAX_INPUT_BYTES] [MAX_PASSAGES].
#                     Defaults to inspecting README.md when no PATH is given.
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

# The workspace lockfile has no third-party packages, so builds stay offline.
ws() {
    subcommand=$1
    shift
    cargo "$subcommand" --manifest-path "$MANIFEST" --locked --offline "$@"
}

action=${1:-run}
[ $# -gt 0 ] && shift
[ "${1:-}" = "--" ] && shift

case $action in
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
        cargo fmt --manifest-path "$MANIFEST" --all -- --check
        ws clippy --workspace --all-targets -- -D warnings
        ws test --workspace
        if [ -d "$PROOFS_DIR" ] && command -v lake >/dev/null 2>&1; then
            (cd "$PROOFS_DIR" && lake build)
        fi
        ;;
    run)
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
        echo "run.sh: unknown command '$action'; use build, fmt, lint, test, check, run, or search" >&2
        exit 1
        ;;
esac
