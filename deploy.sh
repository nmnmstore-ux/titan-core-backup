#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# THE-BRIDGE — One-Command Production Deploy
# Usage:  DOMAIN=bridge.example.com EMAIL=admin@example.com \
#         GRAFANA_PASSWORD=securepass THE_BRIDGE_REGULATOR_SECRET=hex... \
#         ./deploy.sh
# ============================================================

DOMAIN="${DOMAIN:-localhost}"
EMAIL="${EMAIL:-off}"
GRAFANA_PASSWORD="${GRAFANA_PASSWORD:-admin}"
THE_BRIDGE_REGULATOR_SECRET="${THE_BRIDGE_REGULATOR_SECRET:-}"
THE_BRIDGE_TLS_CERT="${THE_BRIDGE_TLS_CERT:-}"
THE_BRIDGE_TLS_KEY="${THE_BRIDGE_TLS_KEY:-}"

echo "==> THE-BRIDGE Deployment — Domain: $DOMAIN"

# Pre-flight checks
if [ -z "$THE_BRIDGE_REGULATOR_SECRET" ]; then
    echo "!! WARNING: THE_BRIDGE_REGULATOR_SECRET not set — an ephemeral keypair will be"
    echo "!! generated on each start. Set it to a persistent hex-encoded 32-byte secret"
    echo "!! for production: export THE_BRIDGE_REGULATOR_SECRET=$(openssl rand -hex 32)"
fi

# 1. Create directories with restricted permissions
mkdir -p certs wal_data
chmod 750 certs wal_data

# 2. Docker deployment
if command -v docker &>/dev/null && command -v docker-compose &>/dev/null; then
    echo "==> Deploying with Docker Compose..."
    DOMAIN="$DOMAIN" EMAIL="$EMAIL" GRAFANA_PASSWORD="$GRAFANA_PASSWORD" \
        THE_BRIDGE_REGULATOR_SECRET="$THE_BRIDGE_REGULATOR_SECRET" \
        docker-compose up --build -d
    echo "==> THE-BRIDGE running. Check: docker-compose ps"
    echo "==> Dashboard: https://$DOMAIN"
    echo "==> Grafana:   http://$DOMAIN:3000 (admin:$GRAFANA_PASSWORD)"
    echo "==> Prometheus: http://$DOMAIN:9090"
    echo "==> API:        http://localhost:3001/api/v1/"
    echo ""
    echo "==> NEXT: Set up heartbeat cron on an independent device:"
    echo "    curl -s -X POST https://$DOMAIN/api/v1/fortress/heartbeat"
    exit 0
fi

# 3. Bare-metal deployment (no Docker)
echo "==> Deploying bare-metal..."

if ! command -v the-bridge-matching-engine &>/dev/null; then
    echo "!! Binary 'the-bridge-matching-engine' not found in PATH"
    echo "!! Build with: cargo build --release"
    exit 1
fi

# Create the-bridge user
if ! id -u the-bridge &>/dev/null; then
    sudo useradd --system --no-create-home --shell /usr/sbin/nologin the-bridge
fi

# Stop existing service before replacing binary
if systemctl is-active --quiet the-bridge 2>/dev/null; then
    echo "==> Stopping existing service..."
    sudo systemctl stop the-bridge
fi

# Backup existing binary
if [ -f /usr/local/bin/the-bridge-matching-engine ]; then
    sudo cp /usr/local/bin/the-bridge-matching-engine /usr/local/bin/the-bridge-matching-engine.bak
    echo "==> Backed up existing binary to .bak"
fi

# Copy binary
sudo cp "$(command -v the-bridge-matching-engine)" /usr/local/bin/
sudo chown the-bridge:the-bridge /usr/local/bin/the-bridge-matching-engine
sudo chmod 750 /usr/local/bin/the-bridge-matching-engine

# WAL directory
sudo mkdir -p /var/lib/the-bridge/wal
sudo chown -R the-bridge:the-bridge /var/lib/the-bridge
sudo chmod 750 /var/lib/the-bridge/wal

# Systemd unit
if [ -f the-bridge.service ]; then
    sudo cp the-bridge.service /etc/systemd/system/
    sudo chmod 644 /etc/systemd/system/the-bridge.service
    sudo systemctl daemon-reload
    sudo systemctl enable the-bridge
    sudo systemctl start the-bridge
    echo "==> THE-BRIDGE started via systemd"
    echo "==> Status: sudo systemctl status the-bridge"
    echo "==> Logs:   sudo journalctl -fu the-bridge"
else
    echo "!! the-bridge.service not found — skipping systemd setup"
fi

# Caddy reverse proxy (optional)
if command -v caddy &>/dev/null && [ "$DOMAIN" != "localhost" ] && [ -f Caddyfile ]; then
    echo "==> Configuring Caddy reverse proxy for $DOMAIN..."
    DOMAIN="$DOMAIN" envsubst < Caddyfile | sudo tee /etc/caddy/Caddyfile >/dev/null
    sudo systemctl enable caddy 2>/dev/null || true
    sudo systemctl restart caddy 2>/dev/null || true
    echo "==> Caddy reverse proxy configured for $DOMAIN"
fi

# ============================================================
# Tor Onion Gateway (Optional — hidden service)
# ============================================================
if command -v tor &>/dev/null; then
    echo "==> Configuring Tor Onion Service..."
    sudo mkdir -p /var/lib/tor/the-bridge
    sudo chown debian-tor:debian-tor /var/lib/tor/the-bridge 2>/dev/null || true
    sudo chmod 700 /var/lib/tor/the-bridge 2>/dev/null || true
    TORRC_ADD="
# THE-BRIDGE Hidden Service
HiddenServiceDir /var/lib/tor/the-bridge
HiddenServicePort 80 127.0.0.1:3001
HiddenServicePort 443 127.0.0.1:3001
"
    if ! grep -q "THE-BRIDGE Hidden Service" /etc/tor/torrc 2>/dev/null; then
        echo "$TORRC_ADD" | sudo tee -a /etc/tor/torrc >/dev/null
    fi
    sudo systemctl enable tor 2>/dev/null || true
    sudo systemctl restart tor 2>/dev/null || true
    echo "==> Waiting for onion address..."
    sleep 3
    ONION=$(sudo cat /var/lib/tor/the-bridge/hostname 2>/dev/null || echo "pending")
    echo "==> Tor Onion Address: $ONION"
    echo "==> Dashboard available at: http://$ONION/dashboard/"
else
    echo "==> Tor not installed. For hidden service: sudo apt install tor"
fi

# ============================================================
# Hot Standby Configuration (Geo-Replica)
# ============================================================
if [ -n "${STANDBY_PEERS:-}" ]; then
    echo "==> Configuring hot standby geo-replica..."
    echo "    Peers: $STANDBY_PEERS"
    mkdir -p /etc/the-bridge
    cat > /tmp/standby.conf << EOF
THE_BRIDGE_PEERS=$STANDBY_PEERS
THE_BRIDGE_NODE_ID=${NODE_ID:-engine-primary}
THE_BRIDGE_REGULATOR_SECRET=${THE_BRIDGE_REGULATOR_SECRET:-}
EOF
    sudo mv /tmp/standby.conf /etc/the-bridge/standby.conf
    sudo chmod 600 /etc/the-bridge/standby.conf
    if [ -f /etc/systemd/system/the-bridge.service ]; then
        sudo mkdir -p /etc/systemd/system/the-bridge.service.d
        cat > /tmp/standby-override.conf << 'EOF'
[Service]
EnvironmentFile=/etc/the-bridge/standby.conf
EOF
        sudo mv /tmp/standby-override.conf /etc/systemd/system/the-bridge.service.d/standby.conf
        sudo systemctl daemon-reload
        sudo systemctl restart the-bridge
    fi
    echo "==> Hot standby configured with $STANDBY_PEERS"
fi

# ============================================================
# Fail2Ban (Optional — rate limiting for failed auth)
# ============================================================
if command -v fail2ban-client &>/dev/null; then
    echo "==> Configuring fail2ban for THE-BRIDGE API..."
    cat > /tmp/the-bridge.conf << 'EOF'
[the-bridge-api]
enabled = true
port = 3001
filter = the-bridge-auth
logpath = /var/log/the-bridge/auth.log
maxretry = 5
bantime = 3600
findtime = 300
EOF
    sudo mkdir -p /etc/fail2ban/jail.d
    sudo mv /tmp/the-bridge.conf /etc/fail2ban/jail.d/the-bridge.conf
    cat > /tmp/the-bridge-filter.conf << 'EOF'
[Definition]
failregex = ^.*invalid API key.*remote=.*$
ignoreregex =
EOF
    sudo mkdir -p /etc/fail2ban/filter.d
    sudo mv /tmp/the-bridge-filter.conf /etc/fail2ban/filter.d/the-bridge-auth.conf
    sudo systemctl restart fail2ban 2>/dev/null || true
    echo "==> fail2ban configured — 5 failed auth attempts = 1h ban"
else
    echo "==> fail2ban not installed. Install with: sudo apt install fail2ban"
fi

# ============================================================
# Firewall (UFW) — restrict access
# ============================================================
if command -v ufw &>/dev/null; then
    echo "==> Applying firewall rules..."
    sudo ufw allow 22/tcp comment 'SSH'
    sudo ufw allow 80/tcp comment 'HTTP (Caddy)'
    sudo ufw allow 443/tcp comment 'HTTPS (Caddy)'
    sudo ufw allow 3001/tcp comment 'THE-BRIDGE API'
    sudo ufw allow 4001/tcp comment 'FIX gateway'
    sudo ufw allow 4002/tcp comment 'DAG consensus'
    # Restrict Grafana/Prometheus to localhost
    sudo ufw deny 3000/tcp comment 'Grafana (internal)'
    sudo ufw deny 9090/tcp comment 'Prometheus (internal)'
    sudo ufw --force enable 2>/dev/null || true
    echo "==> Firewall active: ports 22,80,443,3001,4001,4002 open"
fi

echo ""
echo "================================================"
echo "  THE-BRIDGE — DEPLOYMENT COMPLETE"
echo "================================================"
echo ""
echo "  Dashboard:              https://$DOMAIN/dashboard/"
echo "  Health:                 http://localhost:3001/api/v1/health"
echo "  Heartbeat:              POST http://localhost:3001/api/v1/fortress/heartbeat"
echo "  Audit Trail:            GET  http://localhost:3001/api/v1/fortress/audit"
echo "  Ghost Protocol:         http://localhost:3001/api/v1/ghost/"
echo "  Universal Bridge:       http://localhost:3001/api/v1/bridge/"
echo "  LLM Chat:               POST http://localhost:3001/api/v1/llm/chat"
echo "  Backups:                POST http://localhost:3001/api/v1/backup/trigger"
echo "  Fortress:               http://localhost:3001/api/v1/fortress/"
echo ""
if [ -n "${ONION:-}" ]; then
    echo "  Tor Onion:              http://$ONION/dashboard/"
fi
echo ""
echo "  SECURITY REMINDERS:"
echo "  1. Set THE_BRIDGE_REGULATOR_SECRET for persistent identity"
echo "  2. Configure heartbeat cron on an INDEPENDENT device:"
echo "     echo '* * * * * curl -s -X POST https://$DOMAIN/api/v1/fortress/heartbeat' | crontab -"
echo "  3. Configure succession plan via POST /api/v1/fortress/succession"
echo "  4. Encrypt sovereign files: pwsh SOVEREIGN_ENCRYPT.ps1"
echo "  5. TLS certs go in ./certs/ — set THE_BRIDGE_TLS_CERT and THE_BRIDGE_TLS_KEY"
echo ""
