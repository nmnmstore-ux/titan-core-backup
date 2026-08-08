#!/usr/bin/env bash
# THE-BRIDGE disk hygiene guard (project-safe: never touches sources/.env/release binary)
set -u
PROJ="$HOME/projects/the-bridge"
THRESH_GB=6

free_kb=$(df -Pk / | awk 'NR==2{print $4}')
free_gb=$(( free_kb / 1024 / 1024 ))

# never touch the build dir while a build is running
if pgrep -f 'cargo build' >/dev/null 2>&1; then
    echo "[cleanup] $(date +%FT%T) build in progress, skipped target cleanup (free=${free_gb}GB)"
    exit 0
fi

should_clean=0
if [ -n "${1:-}" ] && [ "$1" = "--auto" ]; then
    [ "$free_gb" -lt "$THRESH_GB" ] && should_clean=1
else
    should_clean=1
fi

if [ "$should_clean" = "1" ]; then
    echo "[cleanup] $(date +%FT%T) free=${free_gb}GB -> cleaning"
    rm -rf "$PROJ/target/debug" 2>/dev/null
    rm -rf "$PROJ/expansion_modules/target" 2>/dev/null
    rm -rf "$PROJ/target/release/examples" 2>/dev/null
    find "$PROJ/target/release/incremental" -type f -mtime +3 -delete 2>/dev/null
    find /tmp -maxdepth 1 -type f -mtime +2 -delete 2>/dev/null
    sudo -n journalctl --vacuum-size=400M >/dev/null 2>&1 || true
    if command -v docker >/dev/null 2>&1; then
        docker image prune -f --filter dangling=true >/dev/null 2>&1 || true
    fi
    sync
    free_kb2=$(df -Pk / | awk 'NR==2{print $4}')
    free_gb2=$(( free_kb2 / 1024 / 1024 ))
    echo "[cleanup] $(date +%FT%T) done, free now=${free_gb2}GB"
else
    echo "[cleanup] $(date +%FT%T) free=${free_gb}GB >= ${THRESH_GB}GB, nothing to do"
fi