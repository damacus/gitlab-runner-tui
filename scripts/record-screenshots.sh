#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCREENSHOTS_DIR="$SCRIPT_DIR/../docs/screenshots"

if [[ "${1:-}" == "--convert" ]]; then
    echo "Converting cast files to GIFs..."
    mkdir -p "$SCREENSHOTS_DIR"
    agg --idle-time-limit 1.5 "$SCREENSHOTS_DIR/overview.cast" "$SCREENSHOTS_DIR/overview.gif"
    agg --idle-time-limit 1.5 "$SCREENSHOTS_DIR/detail.cast" "$SCREENSHOTS_DIR/detail.gif"
    echo "Done. GIFs written to docs/screenshots/"
    echo "File sizes:"
    ls -lh "$SCREENSHOTS_DIR"/*.gif
    exit 0
fi

echo "=== GitLab Runner TUI — Screenshot recorder ==="
echo ""
echo "FIRST: verify your terminal is 130 columns × 40 rows."
echo "  Check: tput cols && tput lines"
echo "  Resize manually to match before recording."
echo ""
echo "Step 1: Record the OVERVIEW gif (navigate all 7 tabs)"
echo "  Run: asciinema rec $SCREENSHOTS_DIR/overview.cast --cols 130 --rows 40 --command 'cargo run -- --demo'"
echo "  In the TUI: press 1, wait 2s, 2, wait 2s ... through 7, then q"
echo ""
echo "Step 2: Record the DETAIL gif (runner detail pane)"
echo "  Run: asciinema rec $SCREENSHOTS_DIR/detail.cast --cols 130 --rows 40 --command 'cargo run -- --demo'"
echo "  In the TUI: use ↑/↓ to arrow through several runners, then q"
echo ""
echo "When both recordings are done, run:"
echo "  $0 --convert"
