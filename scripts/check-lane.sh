#!/usr/bin/env bash
# Fast per-lane check for LeopardCraft agent lanes.
#
# Lane checks are the inner loop for one agent: formatting, clippy and tests
# for that lane's packages only, so an in-progress change in another lane does
# not reformat or fail it. `scripts/check.sh` remains the full integration gate.
#
# Bash 3 compatible (no associative arrays) because the repository's shell
# tooling must also run under macOS and Git Bash.

set -u

cd "$(dirname "${BASH_SOURCE[0]}")/.." || {
    echo "failed to enter the repository root" >&2
    exit 2
}

usage() {
    cat >&2 <<'EOF'
usage: scripts/check-lane.sh <lane>

Lanes:
  geometry     crates/lc-core, crates/lc-geom
  document     crates/lc-anim, crates/lc-scene
  rendering    crates/lc-shading, crates/lc-render
  interchange  crates/lc-io
  editor       crates/lc-ui
  automation   crates/lc-mcp, app (includes the real MCP smoke test)
  all          run scripts/check.sh instead

The lane briefs in docs/agents/ describe each lane's scope and invariants.
EOF
}

case "${1:-}" in
    all)
        exec ./scripts/check.sh
        ;;
    geometry)
        packages=(lc-core lc-geom)
        smoke=0
        ;;
    document)
        packages=(lc-anim lc-scene)
        smoke=0
        ;;
    rendering)
        packages=(lc-shading lc-render)
        smoke=0
        ;;
    interchange)
        packages=(lc-io)
        smoke=0
        ;;
    editor)
        packages=(lc-ui)
        smoke=0
        ;;
    automation)
        packages=(lc-mcp leopardcraft)
        smoke=1
        ;;
    -h|--help|help)
        usage
        exit 0
        ;;
    *)
        if [ "$#" -gt 0 ]; then
            printf 'unknown lane: %s\n' "$1" >&2
        fi
        usage
        exit 2
        ;;
esac

status=0

# Cargo accepts one package per -p flag; build a reusable flag list.
package_flags=()
for package in "${packages[@]}"; do
    package_flags+=(-p "$package")
done

run() {
    printf '\n==> %s\n' "$*"
    if ! "$@"; then
        printf 'FAILED: %s\n' "$*" >&2
        status=1
    fi
}

# Format only this lane's packages; never rewrite another lane's files.
for package in "${packages[@]}"; do
    run cargo fmt -p "$package" -- --check
done

run cargo clippy "${package_flags[@]}" --all-targets -- -D warnings

if [ "$smoke" -eq 1 ]; then
    run cargo test -p lc-mcp
    run cargo test -p leopardcraft --test mcp_smoke
else
    run cargo test "${package_flags[@]}"
fi

if [ "$status" -eq 0 ]; then
    printf '\nLane check passed.\n'
else
    printf '\nLane check failed. Fix the failures above before handoff.\n' >&2
fi

exit "$status"
