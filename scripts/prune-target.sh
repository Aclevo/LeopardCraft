#!/usr/bin/env bash
# Reclaim disk used by build output under target/.
#
# Cargo 1.100's build-dir layout v2 stores each build unit's artifacts only in
# `target/<profile>/build/<package>/<unit-hash>/out` (finals are hardlinked
# into target/<profile>/), but it never deletes a stale unit directory and has
# no target-dir GC, so a few gate runs still add gigabytes. This keeps the
# newest unit directory of each package, drops old incremental sessions and
# sweeps smoke-run litter. Only target/ is touched; the next build recompiles
# whatever was pruned.
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
usage: scripts/prune-target.sh [--dry-run] [--incremental-days N]

  --dry-run             report what would be removed and how much it holds
  --incremental-days N  keep incremental sessions modified within N days
                        (default 3)

Keeps only the newest build-unit directory of each
target/<profile>/build/<package>, and removes incremental sessions older than
N days, target/smoke and stray target/*.log files.
EOF
}

dry_run=0
incremental_days=3

while [ $# -gt 0 ]; do
    case "$1" in
        --dry-run)
            dry_run=1
            ;;
        --incremental-days)
            shift
            case "${1:-}" in
                ''|*[!0-9]*)
                    printf 'invalid day count: %s\n' "${1:-}" >&2
                    usage
                    exit 2
                    ;;
            esac
            incremental_days=$1
            ;;
        -h|--help|help)
            usage
            exit 0
            ;;
        *)
            printf 'unknown argument: %s\n' "$1" >&2
            usage
            exit 2
            ;;
    esac
    shift
done

reclaimed=0

# Kilobyte count as a short human-readable size.
human() {
    awk -v kb="${1:-0}" 'BEGIN {
        if (kb >= 1048576) printf "%.1fG", kb / 1048576
        else if (kb >= 1024) printf "%.0fM", kb / 1024
        else printf "%dK", kb
    }'
}

# Remove one path (or report it under --dry-run) and add its size to the total.
reclaim() {
    [ -e "$1" ] || return 0
    size=$(du -sk "$1" 2>/dev/null | cut -f1)
    size=${size:-0}
    reclaimed=$((reclaimed + size))
    if [ "$dry_run" -eq 1 ]; then
        printf 'would remove  %8s  %s\n' "$(human "$size")" "$1"
    else
        rm -rf "$1"
        printf 'removed       %8s  %s\n' "$(human "$size")" "$1"
    fi
}

# Stale build units: keep the newest unit directory per package.
for package_dir in target/*/build/*; do
    [ -d "$package_dir" ] || continue
    for stale in $(ls -1t "$package_dir" 2>/dev/null | tail -n +2); do
        reclaim "$package_dir/$stale"
    done
done

# Incremental sessions accumulate one directory per changed unit graph.
for stale in $(find target/*/incremental -mindepth 1 -maxdepth 1 -type d \
    -mtime "+$incremental_days" 2>/dev/null); do
    reclaim "$stale"
done

# Litter left by editor smoke runs and by agents redirecting gate output.
reclaim target/smoke
for log in target/*.log; do
    [ -f "$log" ] || continue
    reclaim "$log"
done

if [ "$dry_run" -eq 1 ]; then
    printf '\nWould reclaim %s under target/.\n' "$(human "$reclaimed")"
else
    printf '\nReclaimed %s under target/.\n' "$(human "$reclaimed")"
fi
printf 'target/ now holds %s.\n' "$(du -sh target 2>/dev/null | cut -f1)"
