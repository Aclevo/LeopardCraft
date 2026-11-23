#!/usr/bin/env bash
# Full workspace gate for LeopardCraft.
#
# Runs every check even when one fails so a single invocation reports the whole
# picture. Intended for humans and AI coding agents; CI runs the same build and
# test steps, and `cargo test --workspace` already includes the MCP smoke test.
# The explicit MCP step makes a launch/protocol regression visible in the gate
# output even when the general test step is noisy.

set -u

cd "$(dirname "${BASH_SOURCE[0]}")/.." || {
    echo "failed to enter the repository root" >&2
    exit 2
}

status=0

run() {
    printf '\n==> %s\n' "$*"
    if ! "$@"; then
        printf 'FAILED: %s\n' "$*" >&2
        status=1
    fi
}

run cargo fmt --all -- --check
run cargo clippy --workspace --all-targets -- -D warnings
run cargo test --workspace
run cargo test -p leopardcraft --test mcp_smoke
run git diff --check

if [ "$status" -eq 0 ]; then
    printf '\nAll checks passed.\n'
else
    printf '\nOne or more checks failed.\n' >&2
fi

exit "$status"
