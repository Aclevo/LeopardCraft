#!/usr/bin/env bash
# Dispatch one LeopardCraft lane task packet to whichever agent spawn path the
# environment provides, so callers never branch on the host.
#
# Order of resolution:
#   1. Host spawn_agents bridge. LEOPARDCRAFT_SPAWN_AGENTS names one command
#      (a path or command name, no arguments). It is invoked as
#      "<command> lc-<lane> <packet>" with the packet also on stdin. A host that
#      owns an in-session spawn_agents tool exposes it this way, because a
#      subprocess cannot call a tool that lives inside the agent process.
#   2. Promptable Codebuff CLI. LEOPARDCRAFT_LANE_CLI names the binary
#      (default: codebuff) and is invoked as
#      "<cli> --trust-agents --cwd <repo root> <prompt>". The interactive
#      Freebuff client accepts no prompt argument, so it cannot be this
#      fallback; with only that client installed, use --print-packet and paste
#      the prompt into the session yourself.
#   3. No path. Exit 2 and name the one action that unblocks dispatch.
#
# Bash 3 compatible (no associative arrays) because the repository's shell
# tooling must also run under macOS and Git Bash.

set -u

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)" || exit 2
cd "$root" || exit 2

usage() {
    cat >&2 <<'EOF'
usage: scripts/spawn-lane.sh [--print-packet] <lane> <packet>

lane     geometry | document | rendering | interchange | editor | automation
packet   task packet text, @path to a packet file, or - to read stdin

Environment:
  LEOPARDCRAFT_SPAWN_AGENTS  command that runs the host's spawn_agents tool
  LEOPARDCRAFT_LANE_CLI      promptable CLI binary (default: codebuff)
EOF
}

print_packet=0
positional=()
for argument in "$@"; do
    case "$argument" in
        -) positional[${#positional[@]}]="$argument" ;;
        --print-packet) print_packet=1 ;;
        -h | --help)
            usage
            exit 0
            ;;
        -*)
            echo "error: unknown option '$argument'" >&2
            usage
            exit 2
            ;;
        *) positional[${#positional[@]}]="$argument" ;;
    esac
done

if [ "${#positional[@]}" -ne 2 ]; then
    usage
    exit 2
fi
lane="${positional[0]}"
packet_argument="${positional[1]}"

case "$lane" in
    geometry | document | rendering | interchange | editor | automation) agent="lc-$lane" ;;
    *)
        echo "error: unknown lane '$lane'" >&2
        usage
        exit 2
        ;;
esac

case "$packet_argument" in
    -) packet="$(cat)" ;;
    @*)
        packet_file="${packet_argument#@}"
        if [ ! -f "$packet_file" ]; then
            echo "error: packet file '$packet_file' does not exist" >&2
            exit 2
        fi
        packet="$(cat "$packet_file")"
        ;;
    *) packet="$packet_argument" ;;
esac

case "$packet" in
    *[![:space:]]*) ;;
    *)
        echo "error: the task packet is empty" >&2
        exit 2
        ;;
esac

prompt="Use the $agent agent with this LeopardCraft lane task packet. Read AGENTS.md,
docs/agents/README.md and docs/agents/$lane.md first, stay inside the lane's
paths (or request a handoff), run scripts/check-lane.sh $lane before reporting,
and answer in the handoff format. Do not commit, push or add dependencies.

$packet"

if [ "$print_packet" -eq 1 ]; then
    printf '%s\n' "$prompt"
    exit 0
fi

if [ -n "${LEOPARDCRAFT_SPAWN_AGENTS:-}" ]; then
    printf '%s\n' "$packet" | "$LEOPARDCRAFT_SPAWN_AGENTS" "$agent" "$packet"
    bridge_status=$?
    if [ "$bridge_status" -eq 0 ]; then
        echo "dispatch: spawn_agents ($agent)"
        exit 0
    fi
    echo "warning: the spawn_agents bridge exited $bridge_status; falling back to the CLI" >&2
fi

cli="${LEOPARDCRAFT_LANE_CLI:-codebuff}"
if ! command -v "$cli" >/dev/null 2>&1; then
    echo "error: no agent dispatch path is available for $agent." >&2
    echo "       Set LEOPARDCRAFT_SPAWN_AGENTS to a command that runs the host's spawn_agents" >&2
    echo "       tool, or install and sign in to the promptable Codebuff CLI ('codebuff')." >&2
    echo "       With only the interactive Freebuff client, run" >&2
    echo "       scripts/spawn-lane.sh --print-packet $lane <packet> and paste the prompt there." >&2
    exit 2
fi

"$cli" --trust-agents --cwd "$root" "$prompt"
cli_status=$?
if [ "$cli_status" -ne 0 ]; then
    echo "error: the CLI ('$cli') exited $cli_status; the packet was not dispatched" >&2
    exit "$cli_status"
fi
echo "dispatch: cli $cli ($agent)"
