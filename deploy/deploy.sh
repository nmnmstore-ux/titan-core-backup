#!/bin/bash
set -euo pipefail

# THE-BRIDGE — Production deployment script
# Usage: sudo bash deploy/deploy.sh
# Prerequisites: Docker, docker compose plugin, git

REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_DIR"

echo "=== THE-BRIDGE Deployment ==="

# 1. Linux tuning (bare metal only)
if [[ -f /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor ]]; then
  echo "[1/5] Applying Linux tuning..."
  bash deploy/linux-tune.sh
else
  echo "[1/5] Skipping Linux tuning (container/vm)"
fi

# 2. Generate secrets if not present
if [[ ! -f .env ]]; then
  echo "[2/5] Generating .env with secure secrets..."
  cat > .env <<EOF
NODE_ID=engine-1
DOMAIN=${DOMAIN:-the-bridge.io}
PG_PASSWORD=$(openssl rand -base64 32)
WAL_ENCRYPTION_KEY=$(openssl rand -base64 32)
WAL_SIGNING_SEED=$(openssl rand -hex 32)
GRAFANA_PASSWORD=$(openssl rand -base64 16)
RUST_LOG=the_bridge=info
EOF
  echo "  .env created — edit DOMAIN and secrets before deploying publicly"
else
  echo "[2/5] .env already exists, skipping"
fi

# 3. Create TLS certs dir
mkdir -p certs

# 4. Pull & build
echo "[3/5] Building Docker images..."
docker compose pull
docker compose build

# 5. Start
echo "[4/5] Starting services..."
docker compose up -d

# 6. Health check
echo "[5/5] Waiting for engine to be ready..."
for i in $(seq 1 30); do
  if curl -sf http://localhost:3001/ready >/dev/null 2>&1; then
    echo "  ✅ THE-BRIDGE is ready on port 3001"
    echo ""
    echo "=== Deployment complete ==="
    echo "  Engine:     http://localhost:3001"
    echo "  Docs:       http://localhost:3001/docs"
    echo "  Grafana:    http://localhost:3000"
    echo "  Prometheus: http://localhost:9090"
    echo ""
    echo "Next steps:"
    echo "  1. Point DOMAIN DNS to this server"
    echo "  2. Run: docker compose exec the-bridge /opt/the-bridge/the-bridge-matching-engine --create-admin-key"
    echo "  3. Set up Stripe/Paddle billing keys"
    exit 0
  fi
  echo "  Waiting... ($i)"
  sleep 2
done

echo "  ⚠️  Engine did not become ready within 60s — check logs: docker compose logs -f"
exit 1
