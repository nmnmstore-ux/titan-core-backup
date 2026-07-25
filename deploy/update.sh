#!/usr/bin/env bash
# ================================================================
# THE-BRIDGE — Update Script
# Usage: bash deploy/update.sh
# Pulls latest code, rebuilds, restarts services
# ================================================================
set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; NC='\033[0m'
log()  { echo -e "${CYAN}[$(date +%H:%M:%S)]${NC} $1"; }
ok()   { echo -e "  ${GREEN}✓${NC} $1"; }
warn() { echo -e "  ${YELLOW}⚠${NC} $1"; }
fail() { echo -e "  ${RED}✗${NC} $1"; exit 1; }

PROJECT_DIR="${PROJECT_DIR:-/home/$USER/the-bridge}"
ME_DIR="$PROJECT_DIR/A-core-infrastructure/matching-engine"

cd "$PROJECT_DIR"

log "Pulling latest code..."
git fetch origin
git checkout main
git pull origin main
ok "Code updated ($(git log --oneline -1))"

log "Rebuilding matching-engine..."
export PATH="$HOME/.cargo/bin:$PATH"
cd "$ME_DIR"
cargo build --release --bin matching-engine 2>&1 | tail -3
ok "Build complete"

log "Restarting services..."
sudo systemctl daemon-reload
for svc in the-bridge-engine rpc-proxy; do
  if systemctl is-active "$svc" &>/dev/null; then
    sudo systemctl restart "$svc"
    ok "$svc restarted"
  fi
done

log "Running migrations..."
for sql in "$ME_DIR/migrations"/*.sql; do
  [[ -f "$sql" ]] || continue
  source "$PROJECT_DIR/.env" 2>/dev/null || true
  PGPASSWORD="${DB_PASSWORD:-thebridge2026}" psql -h 127.0.0.1 -p 5433 \
    -U swiftbridge -d swiftbridge -f "$sql" 2>&1 | tail -2 || true
done

ok "Update complete"
