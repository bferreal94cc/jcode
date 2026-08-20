#!/usr/bin/env bash
# Install Continuous Claude v3 into this machine's Claude Code configuration.
#
# Continuous Claude installs GLOBALLY, into ~/.claude. One install covers every
# repository you open with Claude Code on this machine -- there is no per-repo
# install, because every hook in its settings.json resolves through $HOME/.claude.
#
# This script is the non-interactive equivalent of upstream's 13-step wizard
# (`uv run python -m scripts.setup.wizard`). It performs the steps that need no
# prompting and no container runtime: backup, fetch, and the Claude Code
# integration itself. The optional Postgres/Docker stack is left to the wizard.
#
# Usage:
#   ./install-continuous-claude.sh            # install or update
#   ./install-continuous-claude.sh --dry-run  # report what would change
#   ./install-continuous-claude.sh --no-merge # overwrite instead of preserving
#
# Environment:
#   CC_HOME     checkout location   (default ~/.continuous-claude)
#   CLAUDE_DIR  install target      (default ~/.claude)
#   CC_REF      git ref to install  (default main)

set -euo pipefail

REPO_URL="${CC_REPO_URL:-https://github.com/parcadei/Continuous-Claude-v3.git}"
CC_HOME="${CC_HOME:-$HOME/.continuous-claude}"
CLAUDE_DIR="${CLAUDE_DIR:-$HOME/.claude}"
CC_REF="${CC_REF:-main}"

DRY_RUN=0
MERGE=1
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    --no-merge) MERGE=0 ;;
    -h|--help) sed -n '2,/^$/p' "$0" | sed 's/^#\{1,\} \{0,1\}//;s/^#$//'; exit 0 ;;
    *) echo "unknown option: $arg (try --help)" >&2; exit 2 ;;
  esac
done

say()  { printf '\033[1m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[33mwarning:\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }

# ---------------------------------------------------------------- preflight --
say "Checking prerequisites"

command -v git >/dev/null  || die "git is required but not installed."
command -v node >/dev/null || die "node is required but not installed (the hooks are compiled .mjs)."

PY=""
for candidate in python3.13 python3.12 python3.11 python3; do
  if command -v "$candidate" >/dev/null 2>&1 &&
     "$candidate" -c 'import sys; sys.exit(0 if sys.version_info >= (3,11) else 1)' 2>/dev/null; then
    PY="$candidate"; break
  fi
done
[ -n "$PY" ] || die "python 3.11+ is required but was not found."

printf '    git    %s\n'    "$(git --version | awk '{print $3}')"
printf '    node   %s\n'    "$(node --version)"
printf '    python %s (%s)\n' "$("$PY" -c 'import platform;print(platform.python_version())')" "$PY"

command -v docker >/dev/null 2>&1 || command -v podman >/dev/null 2>&1 || \
  warn "no container runtime found. Skills, agents, hooks and rules install fine;
         the optional Postgres-backed memory/continuity stack will not run until
         you install Docker or Podman and run the upstream wizard."

# -------------------------------------------------------------------- fetch --
if [ -d "$CC_HOME/.git" ]; then
  say "Updating existing checkout at $CC_HOME"
  [ "$DRY_RUN" -eq 1 ] || git -C "$CC_HOME" fetch --depth 1 origin "$CC_REF" -q
  [ "$DRY_RUN" -eq 1 ] || git -C "$CC_HOME" checkout -q FETCH_HEAD
else
  say "Cloning Continuous Claude v3 into $CC_HOME"
  [ "$DRY_RUN" -eq 1 ] || git clone --depth 1 --branch "$CC_REF" -q "$REPO_URL" "$CC_HOME"
fi

SOURCE="$CC_HOME/.claude"
if [ "$DRY_RUN" -eq 0 ] && [ ! -d "$SOURCE" ]; then
  die "expected $SOURCE after checkout, but it is missing."
fi

# ------------------------------------------------------------------- backup --
if [ -d "$CLAUDE_DIR" ]; then
  BACKUP="$CLAUDE_DIR.backup.$(date +%Y%m%d_%H%M%S)"
  say "Backing up $CLAUDE_DIR -> $BACKUP"
  [ "$DRY_RUN" -eq 1 ] || cp -a "$CLAUDE_DIR" "$BACKUP"
  echo "    restore with:  rm -rf '$CLAUDE_DIR' && mv '$BACKUP' '$CLAUDE_DIR'"
  export CC_BACKUP="$BACKUP"
else
  say "No existing $CLAUDE_DIR -- clean install"
  export CC_BACKUP=""
fi

if [ "$DRY_RUN" -eq 1 ]; then
  say "Dry run complete. Nothing was written."
  exit 0
fi

# ------------------------------------------------------------------ install --
say "Installing Claude Code integration"

CLAUDE_DIR="$CLAUDE_DIR" MERGE="$MERGE" "$PY" - "$CC_HOME/opc" <<'PY_EOF'
import json, os, sys
from pathlib import Path

opc = Path(sys.argv[1])
sys.path.insert(0, str(opc))

from scripts.setup.claude_integration import (
    analyze_conflicts,
    detect_existing_setup,
    get_opc_integration_source,
    install_opc_integration,
)

target = Path(os.environ["CLAUDE_DIR"])
merge = os.environ.get("MERGE", "1") == "1"
source = get_opc_integration_source()

# Upstream detects the existing setup in `target`, but install_opc_integration
# rmtree's each target subdirectory before the merge loop reads from it -- so
# `merge_user_items` silently preserves nothing on a normal in-place install.
# We scan the backup copy instead, which install never touches.
backup = os.environ.get("CC_BACKUP") or ""
scan_dir = Path(backup) if backup and Path(backup).is_dir() else target

existing = detect_existing_setup(scan_dir)
conflicts = analyze_conflicts(existing, source)
result = install_opc_integration(
    target, source, merge_user_items=merge, existing=existing, conflicts=conflicts
)

if not result.get("success"):
    print(f"install failed: {result.get('error')}", file=sys.stderr)
    sys.exit(1)

for label, key in (
    ("skills", "installed_skills"),
    ("agents", "installed_agents"),
    ("hooks", "installed_hooks"),
    ("rules", "installed_rules"),
    ("servers", "installed_servers"),
    ("scripts", "installed_scripts"),
):
    print(f"    {result.get(key, 0):>4}  {label}")

merged = result.get("merged_items") or []
if merged:
    print(f"    {len(merged):>4}  of your existing items preserved")
PY_EOF

# --------------------------------------------------------------------- done --
say "Installed into $CLAUDE_DIR"
cat <<EOF

Continuous Claude is now active for every repository you open with Claude Code
on this machine. Start a new session for the hooks to load.

Not done by this script:
  * The Postgres-backed memory / continuity daemon needs a container runtime.
    Run the full wizard for it:  cd "$CC_HOME/opc" && uv run python -m scripts.setup.wizard
  * Optional API keys (Perplexity, NIA) live in "$CC_HOME/opc/.env" -- see .env.example.

To undo:  cd "$CC_HOME/opc" && uv run python -m scripts.setup.wizard --uninstall
EOF
