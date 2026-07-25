#!/usr/bin/env bash
# ================================================================
# THE-BRIDGE — Auto-Setup Script
# ================================================================
# Usage: bash deploy/setup.sh [--git-repo URL] [--branch NAME]
#
# Idempotent: safe to run multiple times
# ================================================================
set -euo pipefail

# ── Colors ──────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; NC='\033[0m'
log()  { echo -e "${CYAN}[$(date +%H:%M:%S)]${NC} $1"; }
ok()   { echo -e "  ${GREEN}✓${NC} $1"; }
warn() { echo -e "  ${YELLOW}⚠${NC} $1"; }
fail() { echo -e "  ${RED}✗${NC} $1"; exit 1; }

# ── Config ──────────────────────────────────────────────────────
GIT_REPO="${GIT_REPO:-https://github.com/user/the-bridge.git}"
GIT_BRANCH="${GIT_BRANCH:-main}"
PROJECT_DIR="${PROJECT_DIR:-/home/$USER/the-bridge}"
DEPLOY_DIR="$PROJECT_DIR/deploy"

# Docker
REDIS_PORT="${REDIS_PORT:-6380}"
POSTGRES_PORT="${POSTGRES_PORT:-5433}"
POSTGRES_DB="${POSTGRES_DB:-swiftbridge}"
POSTGRES_USER="${POSTGRES_USER:-swiftbridge}"
POSTGRES_PASS="${POSTGRES_PASS:-thebridge2026}"
REDIS_PASS="${REDIS_PASS:-thebridge2026}"

# Ports
ENGINE_PORT="${ENGINE_PORT:-8080}"
RPC_PORT="${RPC_PORT:-8546}"
ANVIL_PORT="${ANVIL_PORT:-8545}"
METRICS_PORT="${METRICS_PORT:-9090}"
NODE_EXP_PORT="${NODE_EXP_PORT:-9100}"

# Parse args
while [[ $# -gt 0 ]]; do
  case "$1" in
    --git-repo) GIT_REPO="$2"; shift 2 ;;
    --branch)   GIT_BRANCH="$2"; shift 2 ;;
    *)          fail "Unknown arg: $1" ;;
  esac
done

# ── Prerequisites ───────────────────────────────────────────────
check_root() {
  if [[ $EUID -eq 0 ]]; then fail "Do NOT run as root. Use a regular user with sudo."; fi
  if ! command -v sudo &>/dev/null; then fail "sudo not found."; fi
  ok "User $USER has sudo"
}

install_packages() {
  log "Installing system packages..."
  local pkgs=(curl wget git build-essential pkg-config libssl-dev
              ca-certificates gnupg lsb-release ufw jq)
  local to_install=()
  for pkg in "${pkgs[@]}"; do
    if ! dpkg -s "$pkg" &>/dev/null; then to_install+=("$pkg"); fi
  done
  if [[ ${#to_install[@]} -gt 0 ]]; then
    sudo apt-get update -qq && sudo apt-get install -y -qq "${to_install[@]}"
  fi
  ok "System packages installed"
}

# ── Rust ────────────────────────────────────────────────────────
install_rust() {
  if command -v cargo &>/dev/null; then
    ok "Rust already installed: $(cargo --version)"
    return
  fi
  log "Installing Rust..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
  source "$HOME/.cargo/env"
  if ! command -v cargo &>/dev/null; then fail "Rust installation failed"; fi
  ok "Rust installed: $(cargo --version)"
}

# ── Docker ──────────────────────────────────────────────────────
install_docker() {
  if command -v docker &>/dev/null; then
    ok "Docker already installed: $(docker --version)"
    if ! docker ps &>/dev/null; then
      warn "Docker daemon not accessible. Adding user to docker group..."
      sudo usermod -aG docker "$USER"
      warn "Log out and back in OR run: newgrp docker"
    fi
    return
  fi
  log "Installing Docker..."
  curl -fsSL https://get.docker.com | sudo bash
  sudo usermod -aG docker "$USER"
  sudo systemctl enable docker
  sudo systemctl start docker
  if ! command -v docker &>/dev/null; then fail "Docker installation failed"; fi
  ok "Docker installed: $(docker --version)"
}

# ── Foundry (Anvil) ─────────────────────────────────────────────
install_foundry() {
  if command -v anvil &>/dev/null; then
    ok "Foundry already installed: $(anvil --version 2>&1 | head -1)"
    return
  fi
  if [[ -f "$HOME/.foundry/bin/anvil" ]]; then
    export PATH="$HOME/.foundry/bin:$PATH"
    ok "Foundry already installed (cached)"
    return
  fi
  log "Installing Foundry..."
  curl -L https://foundry.paradigm.xyz | bash
  export PATH="$HOME/.foundry/bin:$PATH"
  "$HOME/.foundry/bin/foundryup" 2>/dev/null || true
  if ! command -v anvil &>/dev/null; then
    warn "Foundry install may need manual: source ~/.bashrc && foundryup"
    return
  fi
  ok "Foundry installed"
}

# ── Project ─────────────────────────────────────────────────────
setup_project() {
  if [[ -d "$PROJECT_DIR/.git" ]]; then
    log "Project already exists. Pulling latest..."
    cd "$PROJECT_DIR"
    git fetch origin
    git checkout "$GIT_BRANCH"
    git pull origin "$GIT_BRANCH"
    ok "Project updated"
    return
  fi
  log "Cloning project from $GIT_REPO ..."
  mkdir -p "$(dirname "$PROJECT_DIR")"
  git clone --branch "$GIT_BRANCH" "$GIT_REPO" "$PROJECT_DIR" || {
    warn "Git clone failed. Creating minimal project structure..."
    mkdir -p "$PROJECT_DIR"
    cd "$PROJECT_DIR"
    git init
    git remote add origin "$GIT_REPO"
    create_project_stub
  }
  ok "Project ready at $PROJECT_DIR"
}

create_project_stub() {
  # Only creates structure if git clone failed
  local me="$PROJECT_DIR"
  mkdir -p "$me/src" "$me/core/src" "$me/flash-loan/src" "$me/arbitrage/src"
  mkdir -p "$me/mev-protection/src" "$me/chaos/src" "$me/integration/src"
  mkdir -p "$me/cross-venue-arb/src" "$me/super-arb/src"
  mkdir -p "$me/deploy"
  mkdir -p "$me/G-infrastructure/monitoring/grafana/provisioning/datasources"
}

# ── Docker Containers ───────────────────────────────────────────
run_postgres() {
  if docker ps --format '{{.Names}}' 2>/dev/null | grep -q "^the-bridge-postgres$"; then
    ok "PostgreSQL container already running"
    return
  fi
  log "Starting PostgreSQL container..."
  docker rm -f the-bridge-postgres 2>/dev/null || true
  docker run -d --name the-bridge-postgres \
    --restart unless-stopped \
    -p "127.0.0.1:$POSTGRES_PORT:5432" \
    -e POSTGRES_DB="$POSTGRES_DB" \
    -e POSTGRES_USER="$POSTGRES_USER" \
    -e POSTGRES_PASSWORD="$POSTGRES_PASS" \
    -v "the-bridge-pgdata:/var/lib/postgresql/data" \
    timescale/timescaledb:latest-pg15
  sleep 3
  if docker ps --format '{{.Names}}' | grep -q "^the-bridge-postgres$"; then
    ok "PostgreSQL running on 127.0.0.1:$POSTGRES_PORT"
  else
    warn "PostgreSQL container failed to start. Check: docker logs the-bridge-postgres"
  fi
}

run_redis() {
  if docker ps --format '{{.Names}}' 2>/dev/null | grep -q "^the-bridge-redis$"; then
    ok "Redis container already running"
    return
  fi
  log "Starting Redis container..."
  docker rm -f the-bridge-redis 2>/dev/null || true
  docker run -d --name the-bridge-redis \
    --restart unless-stopped \
    -p "127.0.0.1:$REDIS_PORT:6379" \
    redis:7-alpine \
    redis-server --requirepass "$REDIS_PASS" --appendonly yes
  sleep 2
  if docker ps --format '{{.Names}}' | grep -q "^the-bridge-redis$"; then
    ok "Redis running on 127.0.0.1:$REDIS_PORT"
  else
    warn "Redis container failed to start. Check: docker logs the-bridge-redis"
  fi
}

# ── Nginx ───────────────────────────────────────────────────────
install_nginx() {
  if command -v nginx &>/dev/null; then
    ok "Nginx already installed"
    return
  fi
  log "Installing nginx..."
  sudo apt-get install -y -qq nginx
  sudo systemctl enable nginx
  sudo systemctl start nginx
  ok "Nginx installed"
}

setup_nginx() {
  log "Configuring nginx..."
  local conf="/etc/nginx/sites-available/the-bridge"
  if [[ -f "$conf" ]]; then
    ok "Nginx config already exists"
    return
  fi
  sudo tee "$conf" > /dev/null <<'NGINX'
server {
    listen 80;
    server_name titan-core;
    return 301 https://$host$request_uri;
}
server {
    listen 443 ssl http2;
    server_name titan-core;
    ssl_certificate /etc/ssl/certs/the-bridge.crt;
    ssl_certificate_key /etc/ssl/private/the-bridge.key;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers HIGH:!aNULL:!MD5;
    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    }
    location /rpc/ {
        proxy_pass http://127.0.0.1:8546/;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
NGINX
  sudo ln -sf "$conf" /etc/nginx/sites-enabled/
  sudo mkdir -p /etc/ssl/certs /etc/ssl/private
  if [[ ! -f /etc/ssl/certs/the-bridge.crt ]]; then
    sudo openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
      -keyout /etc/ssl/private/the-bridge.key \
      -out /etc/ssl/certs/the-bridge.crt \
      -subj "/CN=titan-core" 2>/dev/null
  fi
  sudo nginx -t && sudo systemctl reload nginx
  ok "Nginx configured with self-signed SSL"
}

# ── Build ───────────────────────────────────────────────────────
build_engine() {
  if [[ ! -f "$PROJECT_DIR/Cargo.toml" ]]; then
    warn "Cargo.toml not found. Skipping build."
    return
  fi
  log "Building matching-engine (release)..."
  cd "$PROJECT_DIR"
  export PATH="$HOME/.cargo/bin:$PATH"
  cargo build --release --bin matching-engine 2>&1 | tail -5
  if [[ -f "target/release/matching-engine" ]]; then
    sudo cp target/release/matching-engine /usr/local/bin/the-bridge-engine
    ok "Build complete: /usr/local/bin/the-bridge-engine"
  else
    fail "Build failed. Check output above."
  fi
}

# ── Systemd Services ────────────────────────────────────────────
create_systemd_service() {
  local name="$1"
  local file="/etc/systemd/system/$name.service"
  local content="$2"
  if [[ -f "$file" ]]; then
    local old_sum; old_sum=$(md5sum "$file" | cut -d' ' -f1)
    local new_sum; new_sum=$(echo "$content" | md5sum | cut -d' ' -f1)
    if [[ "$old_sum" == "$new_sum" ]]; then
      ok "Service $name already up to date"
      return
    fi
  fi
  echo "$content" | sudo tee "$file" > /dev/null
  ok "Service $name created/updated"
}

setup_services() {
  log "Creating systemd services..."

  create_systemd_service "the-bridge-engine" \
'[Unit]
Description=THE-BRIDGE Matching Engine
After=network.target docker.service
Requires=docker.service

[Service]
Type=simple
Environment=RUST_LOG=info
Environment=API_KEY=the-bridge-api-key-2026
Environment=DATABASE_URL=postgres://'$POSTGRES_USER':'$POSTGRES_PASS'@127.0.0.1:'$POSTGRES_PORT'/'$POSTGRES_DB'
Environment=REDIS_URL=redis://default:'$REDIS_PASS'@127.0.0.1:'$REDIS_PORT'
ExecStart=/usr/local/bin/the-bridge-engine
WorkingDirectory='"$PROJECT_DIR"'
Restart=always
RestartSec=5
User='"$USER"'

[Install]
WantedBy=multi-user.target'

  create_systemd_service "rpc-proxy" \
'[Unit]
Description=THE-BRIDGE RPC Proxy (integrated in matching-engine)
After=network.target

[Service]
Type=oneshot
ExecStart=/bin/true
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target'

  create_systemd_service "anvil" \
'[Unit]
Description=Anvil Local Ethereum Node
After=network.target

[Service]
Type=simple
ExecStart='$HOME'/.foundry/bin/anvil --host 127.0.0.1 --port '"$ANVIL_PORT"' --block-time 2
Restart=always
RestartSec=5
User='"$USER"'

[Install]
WantedBy=multi-user.target'

  sudo systemctl daemon-reload
  log "Enabling services..."
  for svc in the-bridge-engine anvil; do
    if systemctl is-enabled "$svc" &>/dev/null; then
      sudo systemctl enable "$svc" 2>/dev/null || true
    fi
    if systemctl is-active "$svc" &>/dev/null; then
      sudo systemctl restart "$svc" 2>/dev/null || true
    fi
  done
  ok "Systemd services configured"
}

# ── Database Migrations ─────────────────────────────────────────
run_migrations() {
  local mig_dir="$PROJECT_DIR/migrations"
  if [[ ! -d "$mig_dir" ]] || [[ -z "$(ls -A "$mig_dir" 2>/dev/null)" ]]; then
    ok "No migrations to run"
    return
  fi
  log "Running database migrations..."
  for sql in "$mig_dir"/*.sql; do
    [[ -f "$sql" ]] || continue
    log "  Applying: $(basename "$sql")"
    PGPASSWORD="$POSTGRES_PASS" psql -h 127.0.0.1 -p "$POSTGRES_PORT" \
      -U "$POSTGRES_USER" -d "$POSTGRES_DB" -f "$sql" 2>&1 | tail -3 || warn "Migration failed for $(basename $sql)"
  done
  ok "Migrations applied"
}

# ── .env ────────────────────────────────────────────────────────
setup_env() {
  local env_file="$PROJECT_DIR/.env"
  if [[ -f "$env_file" ]]; then
    ok ".env already exists"
    return
  fi
  cat > "$env_file" <<EOF
# THE-BRIDGE Environment (auto-generated by setup.sh)
REAL_EXECUTION=false
SUPER_ARB_ENABLED=true
MEV_ENABLED=false
FLASH_LOAN_ENABLED=false
CROSS_VENUE_ENABLED=false

ETH_RPC_URL=https://eth-mainnet.g.alchemy.com/v2/demo
BSC_RPC_URL=https://bsc-dataseed.binance.org
POLYGON_RPC_URL=https://polygon-rpc.com
ARBITRUM_RPC_URL=https://arb1.arbitrum.io/rpc
OPTIMISM_RPC_URL=https://mainnet.optimism.io
BASE_RPC_URL=https://base.llamarpc.com

DATABASE_URL=postgres://$POSTGRES_USER:$POSTGRES_PASS@127.0.0.1:$POSTGRES_PORT/$POSTGRES_DB
REDIS_URL=redis://default:$REDIS_PASS@127.0.0.1:$REDIS_PORT
API_KEY=the-bridge-api-key-2026

SUPER_SCAN_INTERVAL_MS=1500
MEV_SCAN_INTERVAL_MS=1000
FL_SCAN_INTERVAL_MS=2000
CV_SCAN_INTERVAL_MS=2000

SUPER_MIN_PROFIT_USD=10.0
MEV_MIN_PROFIT_USD=100.0
FL_MIN_PROFIT_USD=50.0
CV_MIN_PROFIT_USD=10.0

SUPER_MAX_TRADE_SIZE_USD=10000.0
CV_MAX_TRADE_SIZE_USD=10000.0
SUPER_MAX_CONCURRENT=5
FL_MAX_CONCURRENT=3
MEV_MAX_CONCURRENT_BUNDLES=3

FL_MIN_PROFIT_BPS=15
CV_MIN_PROFIT_BPS=5.0
EOF
  chmod 600 "$env_file"
  ok ".env created"
}

# ── Firewall ────────────────────────────────────────────────────
setup_firewall() {
  if ! command -v ufw &>/dev/null; then return; fi
  log "Configuring firewall..."
  sudo ufw --force reset 2>/dev/null || true
  sudo ufw default deny incoming
  sudo ufw default allow outgoing
  sudo ufw allow 2222/tcp comment 'SSH'
  sudo ufw allow 80/tcp comment 'HTTP'
  sudo ufw allow 443/tcp comment 'HTTPS'
  sudo ufw allow 3389/tcp comment 'RDP'
  sudo ufw --force enable 2>/dev/null || true
  ok "Firewall configured (SSH, HTTP, HTTPS, RDP)"
}

# ── Monitoring ──────────────────────────────────────────────────
setup_monitoring() {
  local mon_dir="$PROJECT_DIR/G-infrastructure/monitoring"
  mkdir -p "$mon_dir/grafana/provisioning/datasources"

  # prometheus.yml
  if [[ ! -f "$mon_dir/prometheus.yml" ]]; then
    cat > "$mon_dir/prometheus.yml" <<'PROM'
global:
  scrape_interval: 5s
scrape_configs:
  - job_name: "matching-engine"
    static_configs:
      - targets: ["127.0.0.1:9000"]
        labels: { service: matching-engine }
PROM
  fi

  # alertmanager.yml
  if [[ ! -f "$mon_dir/alertmanager.yml" ]]; then
    cat > "$mon_dir/alertmanager.yml" <<'ALERT'
global:
  resolve_timeout: 5m
route:
  group_by: ['alertname']
  group_wait: 10s
  group_interval: 30s
  repeat_interval: 4h
  receiver: 'default'
receivers:
  - name: 'default'
ALERT
  fi

  # Grafana datasource
  if [[ ! -f "$mon_dir/grafana/provisioning/datasources/prometheus.yml" ]]; then
    cat > "$mon_dir/grafana/provisioning/datasources/prometheus.yml" <<'GRAF'
apiVersion: 1
datasources:
  - name: Prometheus
    type: prometheus
    access: proxy
    url: http://127.0.0.1:9090
    isDefault: true
GRAF
  fi

  ok "Monitoring configs created"
}

# ── Docker Compose ──────────────────────────────────────────────
setup_docker_compose() {
  local compose="$PROJECT_DIR/docker-compose.yml"
  if [[ -f "$compose" ]]; then
    ok "docker-compose.yml already exists"
    return
  fi
  cat > "$compose" <<'YML'
networks:
  swiftbridge:
    driver: bridge
volumes:
  postgres_data:
  redis_data:
services:
  postgres:
    image: timescale/timescaledb:latest-pg15
    environment:
      POSTGRES_DB: swiftbridge
      POSTGRES_USER: swiftbridge
      POSTGRES_PASSWORD: thebridge2026
    ports: ["127.0.0.1:5433:5432"]
    volumes: [postgres_data:/var/lib/postgresql/data]
    networks: [swiftbridge]
    restart: unless-stopped
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U swiftbridge"]
      interval: 10s
      timeout: 5s
      retries: 5
  redis:
    image: redis:7-alpine
    command: redis-server --requirepass thebridge2026 --appendonly yes
    ports: ["127.0.0.1:6380:6379"]
    volumes: [redis_data:/data]
    networks: [swiftbridge]
    restart: unless-stopped
YML
  ok "docker-compose.yml created"
}

# ── Verification ────────────────────────────────────────────────
verify() {
  local errors=0
  log "Running verification..."

  # Services
  for svc in the-bridge-engine anvil; do
    if systemctl is-active "$svc" &>/dev/null; then
      ok "  $svc: active"
    else
      warn "  $svc: inactive"
      ((errors++))
    fi
  done

  # Docker
  if docker ps --format '{{.Names}}' 2>/dev/null | grep -q "^the-bridge-postgres$"; then
    ok "  PostgreSQL: running"
  else
    warn "  PostgreSQL: not running"
    ((errors++))
  fi
  if docker ps --format '{{.Names}}' 2>/dev/null | grep -q "^the-bridge-redis$"; then
    ok "  Redis: running"
  else
    warn "  Redis: not running"
    ((errors++))
  fi

  # Nginx
  if systemctl is-active nginx &>/dev/null; then
    ok "  nginx: active"
  else
    warn "  nginx: inactive"
    ((errors++))
  fi

  # HTTP endpoints
  if command -v curl &>/dev/null; then
    if curl -sf http://127.0.0.1:"$ENGINE_PORT"/health &>/dev/null; then
      ok "  Engine API: responding"
    else
      warn "  Engine API: not responding"
      ((errors++))
    fi
  fi

  if [[ $errors -eq 0 ]]; then
    echo -e "\n${GREEN}═══════════════════════════════════════════${NC}"
    echo -e "${GREEN}  ✓ THE-BRIDGE fully operational!${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════${NC}"
    echo "  Engine:   http://127.0.0.1:$ENGINE_PORT"
    echo "  RPC:      http://127.0.0.1:$RPC_PORT"
    echo "  HTTPS:    https://<server-ip>/"
    echo "  RPC/HTTPS:https://<server-ip>/rpc/"
    echo "  Postgres: 127.0.0.1:$POSTGRES_PORT"
    echo "  Redis:    127.0.0.1:$REDIS_PORT"
    echo "  SSH:      port 2222"
    echo "  RDP:      port 3389"
  else
    echo -e "\n${YELLOW}⚠ $errors checks failed. Review warnings above.${NC}"
  fi
}

# ── Main ────────────────────────────────────────────────────────
main() {
  echo -e "${CYAN}═══════════════════════════════════════════${NC}"
  echo -e "${CYAN}  THE-BRIDGE Auto-Setup${NC}"
  echo -e "${CYAN}═══════════════════════════════════════════${NC}"

  check_root
  install_packages
  install_rust
  install_docker
  install_foundry
  install_nginx

  setup_project
  setup_env
  setup_docker_compose

  run_postgres
  run_redis
  setup_nginx
  setup_monitoring
  setup_firewall

  build_engine
  setup_services

  # Wait for containers
  log "Waiting for PostgreSQL to be ready..."
  for i in $(seq 1 15); do
    if PGPASSWORD="$POSTGRES_PASS" psql -h 127.0.0.1 -p "$POSTGRES_PORT" \
      -U "$POSTGRES_USER" -d "$POSTGRES_DB" -c "SELECT 1" &>/dev/null; then
      ok "PostgreSQL ready"
      run_migrations
      break
    fi
    sleep 2
  done

  verify
  echo -e "\n${CYAN}═══════════════════════════════════════════${NC}"
  echo -e "${CYAN}  Setup complete!${NC}"
  echo -e "${CYAN}  SSH on port 2222 | RDP on port 3389${NC}"
  echo -e "${CYAN}═══════════════════════════════════════════${NC}"
}

main "$@"
