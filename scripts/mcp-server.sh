#!/usr/bin/env bash
# Launch the LeopardCraft MCP server from any working directory.
#
# The project .mcp.json resolves this script through
# ${CLAUDE_PROJECT_DIR:-.}. The wrapper then pins the workspace manifest and
# package, changes to the repository root, and runs the freshly built binary.
#
# LEOPARDCRAFT_MCP_BIN overrides the binary path for packaging or alternate builds.

set -u

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)" || exit 1
cd "$root" || exit 1

if [ -n "${LEOPARDCRAFT_MCP_BIN:-}" ]; then
    binary="$LEOPARDCRAFT_MCP_BIN"
else
    target_dir="${CARGO_TARGET_DIR:-$root/target}"
    case "$target_dir" in
        /*) ;;
        *) target_dir="$root/$target_dir" ;;
    esac

    binary="$target_dir/debug/leopardcraft"
    if [ -x "${binary}.exe" ]; then
        binary="${binary}.exe"
    fi

    watch_paths=("$root/app" "$root/crates" "$root/Cargo.toml")
    if [ -f "$root/Cargo.lock" ]; then
        watch_paths+=("$root/Cargo.lock")
    fi

    needs_build=0
    if [ ! -x "$binary" ]; then
        needs_build=1
    elif find "${watch_paths[@]}" \
        \( -type d -name tests -o -type d -name benches \) -prune -o \
        -newer "$binary" -print -quit | grep -q .; then
        needs_build=1
    fi

    if [ "$needs_build" -eq 1 ]; then
        printf 'Building LeopardCraft MCP server...\n' >&2
        cargo build --quiet --manifest-path "$root/Cargo.toml" -p leopardcraft 1>&2 || exit 1
    fi
fi

exec "$binary" --mcp "$@"
