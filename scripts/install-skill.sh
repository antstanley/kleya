#!/bin/sh
# install-skill.sh — install the using-kleya Claude Code skill across agents.
#
# Reads agent home directories under $HOME to decide where to install:
#   ~/.claude/         -> ~/.claude/skills/using-kleya/SKILL.md
#   ~/.cursor/         -> ~/.cursor/skills/using-kleya/SKILL.md
#   ~/.config/opencode/-> ~/.config/opencode/skills/using-kleya/SKILL.md
#   ~/.agents/         -> ~/.agents/skills/using-kleya/SKILL.md
#   ~/.codex/          -> ~/.codex/AGENTS.md  (appended between markers)
#
# If none are detected the installer falls back to ~/.claude/ — that's the
# most common target for the kleya audience and matches the upstream
# Claude Code convention.
#
# Usage:
#   install-skill.sh [--target=<list>] [--version=<tag>] [--dry-run]
#   curl -fsSL <release>/install-skill.sh | sh
#   curl -fsSL <release>/install-skill.sh | sh -s -- --target=claude,opencode
#
# --target accepts a comma-separated list or "all". Override autodetection
# when you know which agents you have, or to deliberately fan out wider.

set -eu

REPO="antstanley/kleya"
SKILL_NAME="using-kleya"
# Filled in at release time by the release-skill workflow. Defaults to
# "latest" so a script bundled into a release still works when run directly
# without substitution (e.g. from a local clone during development).
VERSION_DEFAULT="__VERSION__"

TARGET_LIST=""
VERSION="$VERSION_DEFAULT"
DRY_RUN=0

print_help() {
    cat <<'EOF'
install-skill.sh — install the using-kleya skill across coding agents.

Options:
  --target=<list>     comma-separated subset of: claude, cursor, opencode, agents, codex, all
                      (default: autodetect from $HOME/.<dir>)
  --version=<tag>     pin to a specific release tag (default: latest published)
  --dry-run           print what would be installed without touching the filesystem
  -h, --help          show this message
EOF
}

log() { printf '  %s\n' "$*" >&2; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

for arg in "$@"; do
    case "$arg" in
        --target=*)  TARGET_LIST="${arg#*=}" ;;
        --version=*) VERSION="${arg#*=}" ;;
        --dry-run)   DRY_RUN=1 ;;
        -h|--help)   print_help; exit 0 ;;
        *)           die "unknown argument: $arg (try --help)" ;;
    esac
done

# If the script wasn't substituted at release time, treat as latest.
case "$VERSION" in
    __VERSION__|"") VERSION="latest" ;;
esac

# ---------------------------------------------------------------------------
# Resolve targets
# ---------------------------------------------------------------------------

resolve_targets() {
    if [ -z "$TARGET_LIST" ]; then
        detected=""
        [ -d "$HOME/.claude" ]          && detected="${detected:+$detected }claude"
        [ -d "$HOME/.cursor" ]          && detected="${detected:+$detected }cursor"
        [ -d "$HOME/.config/opencode" ] && detected="${detected:+$detected }opencode"
        [ -d "$HOME/.agents" ]          && detected="${detected:+$detected }agents"
        [ -d "$HOME/.codex" ]           && detected="${detected:+$detected }codex"
        if [ -z "$detected" ]; then
            warn "no agent directories detected under \$HOME — defaulting to claude"
            detected="claude"
        fi
        echo "$detected"
    elif [ "$TARGET_LIST" = "all" ]; then
        echo "claude cursor opencode agents codex"
    else
        # Translate commas to spaces.
        echo "$TARGET_LIST" | tr ',' ' '
    fi
}

TARGETS=$(resolve_targets)

# ---------------------------------------------------------------------------
# Download + extract the skill artifact
# ---------------------------------------------------------------------------

require_cmd() { command -v "$1" >/dev/null 2>&1 || die "$1 is required but not on PATH"; }
require_cmd curl
require_cmd unzip
require_cmd awk
require_cmd mkdir
require_cmd cp

if [ "$VERSION" = "latest" ]; then
    URL="https://github.com/$REPO/releases/latest/download/$SKILL_NAME.skill"
else
    URL="https://github.com/$REPO/releases/download/$VERSION/$SKILL_NAME.skill"
fi

TMP=$(mktemp -d 2>/dev/null || mktemp -d -t kleya-skill)
trap 'rm -rf "$TMP"' EXIT INT TERM

log "downloading $URL"
if [ "$DRY_RUN" -eq 0 ]; then
    curl --proto '=https' --tlsv1.2 -fsSL "$URL" -o "$TMP/skill.zip" \
        || die "failed to download $URL"
    unzip -q "$TMP/skill.zip" -d "$TMP/extracted" \
        || die "failed to unzip $TMP/skill.zip"
fi
SKILL_SRC="$TMP/extracted/$SKILL_NAME"

# ---------------------------------------------------------------------------
# Install helpers
# ---------------------------------------------------------------------------

# Strip a leading YAML frontmatter block (delimited by `---` lines) from $1
# on stdout. If there's no frontmatter, the file passes through unchanged.
strip_frontmatter() {
    awk '
        BEGIN { in_fm = 0; saw_fm = 0; line = 0 }
        {
            line++
            if (line == 1 && $0 == "---") { in_fm = 1; saw_fm = 1; next }
            if (in_fm && $0 == "---")     { in_fm = 0; next }
            if (in_fm)                    { next }
            print
        }
    ' "$1"
}

install_skill_dir() {
    label="$1"
    dest_dir="$2"
    dest="$dest_dir/SKILL.md"
    log "$label: installing to $dest"
    if [ "$DRY_RUN" -eq 0 ]; then
        mkdir -p "$dest_dir"
        cp "$SKILL_SRC/SKILL.md" "$dest"
    fi
}

install_codex() {
    agents="$HOME/.codex/AGENTS.md"
    marker_start="<!-- BEGIN: $SKILL_NAME skill (managed by install-skill.sh) -->"
    marker_end="<!-- END: $SKILL_NAME skill -->"
    log "codex: updating $agents (between idempotent markers)"
    if [ "$DRY_RUN" -eq 1 ]; then return; fi
    mkdir -p "$HOME/.codex"
    tmp=$(mktemp)
    if [ -f "$agents" ]; then
        # Drop any existing managed block AND any trailing blank lines, so
        # the rebuild produces byte-identical output on every run.
        awk -v s="$marker_start" -v e="$marker_end" '
            $0 == s { skip = 1; next }
            $0 == e { skip = 0; next }
            skip    { next }
            /./     { for (i = 0; i < bc; i++) print ""; bc = 0; print; next }
                    { bc++ }
        ' "$agents" > "$tmp"
    fi
    {
        if [ -s "$tmp" ]; then echo; fi
        echo "$marker_start"
        strip_frontmatter "$SKILL_SRC/SKILL.md"
        echo "$marker_end"
    } >> "$tmp"
    mv "$tmp" "$agents"
}

# ---------------------------------------------------------------------------
# Run
# ---------------------------------------------------------------------------

log "kleya skill installer (version=$VERSION, targets:$TARGETS)"

for target in $TARGETS; do
    case "$target" in
        claude)   install_skill_dir "claude"   "$HOME/.claude/skills/$SKILL_NAME" ;;
        cursor)   install_skill_dir "cursor"   "$HOME/.cursor/skills/$SKILL_NAME" ;;
        opencode) install_skill_dir "opencode" "$HOME/.config/opencode/skills/$SKILL_NAME" ;;
        agents)   install_skill_dir "agents"   "$HOME/.agents/skills/$SKILL_NAME" ;;
        codex)    install_codex ;;
        "")       ;;
        *)        warn "unknown target '$target' (skipping)" ;;
    esac
done

log "done. restart your agent for changes to take effect."
