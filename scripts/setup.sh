#!/usr/bin/env bash
# ============================================================
# THE-BRIDGE — One-Command Server Bootstrap
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/yourorg/the-bridge/main/scripts/setup.sh | bash
#   # or locally:
#   sudo bash scripts/setup.sh
# ============================================================
set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
info()  { echo -e "${GREEN}[INFO]${NC}  $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
err()   { echo -e "${RED}[ERROR]${NC} $*"; }

# ===== 1. Prerequisites =====
info "Checking prerequisites..."
if [[ $EUID -ne 0 ]]; then err "Run as root: sudo bash $0"; exit 1; fi

command -v docker &>/dev/null || { err "Docker required. Install: curl -fsSL https://get.docker.com | bash"; exit 1; }
command -v docker-compose &>/dev/null || docker compose version &>/dev/null || {
    warn "docker-compose plugin not found, installing..."
    apt update && apt install -y docker-compose-plugin
}

# ===== 2. Performance Tuning =====
info "Tuning kernel parameters..."
cpupower frequency-set -g performance 2>/dev/null || warn "cpupower not available (install linux-tools)"
echo 2048 > /proc/sys/vm/nr_hugepages 2>/dev/null || warn "hugepages config failed (non-NUMA system?)"
sysctl -w net.core.rmem_max=134217728 net.core.wmem_max=134217728 net.core.netdev_budget=600 vm.swappiness=1 2>/dev/null || true

# ===== 3. Data Directories =====
info "Creating data directories..."
mkdir -p /var/lib/the-bridge/{wal,iso20022}
chmod 700 /var/lib/the-bridge/wal

# ===== 4. Certs (self-signed for initial bootstrap) =====
if [[ ! -f /etc/the-bridge/certs/cert.pem ]]; then
    info "Generating self-signed cert for initial setup..."
    mkdir -p /etc/the-bridge/certs
    openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
        -keyout /etc/the-bridge/certs/key.pem \
        -out /etc/the-bridge/certs/cert.pem \
        -subj "/CN=the-bridge.local" 2>/dev/null
fi

# ===== 5. Docker Network =====
info "Setting up Docker network..."
docker network inspect the-bridge-net &>/dev/null || docker network create the-bridge-net

# ===== 6. Verify Engine Binary =====
if command -v the-bridge-matching-engine &>/dev/null; then
    info "Binary found: $(which the-bridge-matching-engine)"
    info "Version: $(the-bridge-matching-engine --version 2>/dev/null || echo 'check binary')"
else
    warn "No pre-installed binary — will use Docker image"
fi

# ===== 7. Summary =====
echo ""
info "========================================="
info "  THE-BRIDGE — Server Bootstrap Complete"
info "========================================="
echo ""
info "Next steps:"
info "  1. Create .env file:"
info "       cp .env.example .env && nano .env"
info "  2. Start services:"
info "       docker compose up -d"
info "  3. Check health:"
info "       curl -f https://yourdomain.com/api/v1/health"
echo ""
info "Ports:"
info "   443  → HTTPS (Caddy → Engine)"
info "   3000 → Grafana (admin:yourpassword)"
info "   9090 → Prometheus"
echo ""

# ===== 8. Verify NUMA =====
if command -v numactl &>/dev/null; then
    echo "NUMA topology:"
    numactl --hardware 2>/dev/null || echo "  (single node)"
else
    warn "numactl not installed — for 1.5M TPS, install: apt install numactl"
fi
