#!/usr/bin/env sh
# Build, check, and launch the runnable OIA code that exists on main.
#
# There is no root Cargo workspace or OIA application binary yet. The only
# runnable entry point is the read-only `inspect-text` example of the
# standalone `crates/oia-documents` package (DOC-02 slice of #17). Update this
# script when the P0 workspace (#9) lands.
#
# Usage: scripts/run.sh [build|check|run] [-- ARGS...]
#   build          Build the package and its inspection example.
#   check          Run the package-local fmt, clippy, and test checks.
#   run [ARGS...]  Run inspect-text: PATH [MAX_INPUT_BYTES] [MAX_PASSAGES].
#                  Defaults to inspecting README.md when no PATH is given.
#
# Environment: OIA_RUST_TOOLCHAIN overrides the pinned toolchain (1.99.0).
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
MANIFEST="$ROOT/crates/oia-documents/Cargo.toml"
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

# The package lockfile has no third-party packages, so builds stay offline.
pkg() {
    subcommand=$1
    shift
    cargo "$subcommand" --manifest-path "$MANIFEST" --locked --offline "$@"
}

action=${1:-run}
[ $# -gt 0 ] && shift
[ "${1:-}" = "--" ] && shift

case $action in
    build)
        pkg build --all-targets
        ;;
    check)
        cargo fmt --manifest-path "$MANIFEST" --all -- --check
        pkg clippy --workspace --all-targets -- -D warnings
        pkg test --workspace
        ;;
    run)
        [ $# -eq 0 ] && set -- "$ROOT/README.md"
        # Exit codes pass through: 0 complete/empty, 2 partial, 1 error.
        pkg run --quiet --example inspect-text -- "$@"
        ;;
    -h|--help|help)
        sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'
        ;;
    *)
        echo "run.sh: unknown command '$action'; use build, check, or run" >&2
        exit 1
        ;;
esac
