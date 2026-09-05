#!/usr/bin/env bash
# Measures what Safe Invest actually costs to run.
#
# Reports binary size, cold start time and peak resident memory for the two
# things the executable does: opening the window, and answering MCP calls.
# Run it before and after a change that could plausibly cost something.
#
#     ./scripts/profile.sh            # release build, current tree
#     ./scripts/profile.sh --build    # build first
#
# On Linux the window is measured under a virtual display, so the numbers are
# comparable between a laptop and a CI runner but are not Windows numbers.

set -euo pipefail

cd "$(dirname "$0")/.."
BIN=target/release/safe-invest
DATA=$(mktemp -d)
trap 'rm -rf "$DATA"' EXIT

if [[ "${1:-}" == "--build" ]]; then
    cargo build --release --locked
fi

if [[ ! -x "$BIN" ]]; then
    echo "Compilez d'abord : cargo build --release" >&2
    exit 1
fi

section() { printf '\n\033[1m%s\033[0m\n' "$1"; }
row() { printf '  %-34s %s\n' "$1" "$2"; }

# ------------------------------------------------------------------ size

section "Taille"
row "exécutable" "$(du -h "$BIN" | cut -f1)"
row "dont ressources embarquées" "$(du -sh crates/app/ui | cut -f1) (interface)"

# ------------------------------------------------------- console startup

section "Démarrage — sous-commandes console"
for command in --version "doctor --demo"; do
    total=0
    runs=10
    for _ in $(seq $runs); do
        start=$(date +%s%N)
        # shellcheck disable=SC2086
        "$BIN" $command --data-dir "$DATA" >/dev/null 2>&1 || true
        total=$(( total + ($(date +%s%N) - start) / 1000000 ))
    done
    row "$command (moyenne sur $runs)" "$(( total / runs )) ms"
done

# ------------------------------------------------------------- MCP round

section "Serveur MCP"
python3 scripts/profile_mcp.py "$BIN" "$DATA"

# ----------------------------------------------------------- window cost

if command -v xvfb-run >/dev/null 2>&1; then
    section "Fenêtre (affichage virtuel)"
    echo "  RSS anonyme = mémoire réellement allouée par le processus."
    echo "  RSS total   = idem plus les pages de bibliothèques partagées, comptées"
    echo "                dans chaque processus qui les projette."
    echo

    xvfb-run -a --server-args="-screen 0 1280x900x24" \
        "$BIN" --data-dir "$DATA" --demo >/dev/null 2>&1 &
    launcher=$!
    sleep 8

    # Only the application and what it starts. The X server that xvfb-run
    # brings up is test scaffolding, not something a user pays for.
    app=$(pgrep -x safe-invest | head -1 || true)
    if [[ -n "$app" ]]; then
        processes="$app"
        frontier="$app"
        while [[ -n "$frontier" ]]; do
            next=""
            for parent in $frontier; do
                # `pgrep` exits 1 for a process with no children, and with
                # `pipefail` that status would end the script.
                children=$(pgrep -P "$parent" 2>/dev/null | tr '\n' ' ' || true)
                next="$next $children"
            done
            frontier=$(echo "$next" | xargs || true)
            processes="$processes $frontier"
        done

        anon_total=0
        rss_total=0
        printf '  %-24s %10s %10s\n' "processus" "RSS anon." "RSS total"
        for process in $processes; do
            status="/proc/$process/status"
            [[ -r "$status" ]] || continue
            anon=$(awk '/^RssAnon:/ {print $2}' "$status" 2>/dev/null || echo 0)
            rss=$(awk '/^VmRSS:/ {print $2}' "$status" 2>/dev/null || echo 0)
            [[ "${rss:-0}" -eq 0 ]] && continue
            name=$(tr -d '\0' < "/proc/$process/comm" 2>/dev/null || echo "?")
            printf '  %-24s %7s Mio %7s Mio\n' "$name" "$(( anon / 1024 ))" "$(( rss / 1024 ))"
            anon_total=$(( anon_total + anon ))
            rss_total=$(( rss_total + rss ))
        done
        printf '  %-24s %7s Mio %7s Mio\n' "TOTAL" "$(( anon_total / 1024 ))" "$(( rss_total / 1024 ))"

        echo
        echo "  Le gros poste est le moteur web du système, pas le code de Safe Invest."
        echo "  Ici c'est WebKitGTK en rendu logiciel ; sur Windows c'est WebView2, dont"
        echo "  l'empreinte diffère. Mesurez sur Windows avant de conclure."
    else
        row "mémoire résidente" "processus introuvable"
    fi
    kill "$launcher" 2>/dev/null || true
    wait "$launcher" 2>/dev/null || true
else
    section "Fenêtre"
    row "ignorée" "xvfb-run absent"
fi

echo
