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
failed=()

run() {
    printf '\n==> %s\n' "$*"
    if ! "$@"; then
        printf 'FAILED: %s\n' "$*" >&2
        failed+=("$*")
        status=1
    fi
}

run cargo fmt --all -- --check
run cargo clippy --workspace --all-targets -- -D warnings
run cargo test --workspace
run cargo test -p leopardcraft --test mcp_smoke
# Check the working tree and the index: `git diff --check` alone ignores
# changes that are already staged for commit.
run git diff --check
run git diff --cached --check

if [ "$status" -eq 0 ]; then
    printf '\nAll checks passed.\n'
else
    printf '\nFailed checks:\n' >&2
    for check in "${failed[@]}"; do
        printf '  - %s\n' "$check" >&2
    done
    printf 'One or more checks failed.\n' >&2
fi

exit "$status"
