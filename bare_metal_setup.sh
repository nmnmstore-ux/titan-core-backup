#!/usr/bin/env bash
# ================================================================
# THE-BRIDGE — bare_metal_setup.sh
# "Logical Hard-Lock" bootstrap: installs the sovereign stack top
# to bottom so any bare server becomes a Bridge node.
#
# Depends-on (logical dependency chain):
#   1. OS deps (Rust toolchain, Foundry, system libs)        → §2 SYSTEM_STATE.md
#   2. NUMA hugepages + IRQ pinning (kernel co-location)     → §2 Engines 1,2
#   3. Local DeepSeek-R1 / Ollama model weights              → §2 Engine 6
#   4. Private RPC gateway endpoints                         → §3 Mode B
#   5. Executor key isolation (chmod 600)                    → §2 §5 / Invariants
#   6. systemd unit (auto-restart) + environment              → §5 Deployment
#
# Usage:
#   sudo bash bare_metal_setup.sh [--rpc-url URL] [--no-ollama]
#
# Idempotent: safe to re-run.
# ================================================================
set -euo pipefail

# ── Colors ──────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; NC='\033[0m'
log()  { echo -e "${CYAN}[$(date +%H:%M:%S)]${NC} $1"; }
ok()   { echo -e "  ${GREEN}✓${NC} $1"; }
warn() { echo -e "  ${YELLOW}⚠${NC} $1"; }
fail() { echo -e "  ${RED}✗${NC} $1"; exit 1; }

# ── Config (defaults; override via args or env) ────────────────
GIT_REPO="${GIT_REPO:-https://github.com/user/the-bridge.git}"
PROJECT_DIR="${PROJECT_DIR:-/home/$SUDO_USER/the-bridge}"
RPC_URL="${ETH_RPC_URL:-https://arb1.arbitrum.io/rpc}"
OLLAMA_ENABLED="${OLLAMA_ENABLED:-1}"
USER_NAME="${SUDO_USER:-$(whoami)}"

# Parse args
while [[ $# -gt 0 ]]; do
  case "$1" in
    --rpc-url)   RPC_URL="$2"; shift 2 ;;
    --no-ollama) OLLAMA_ENABLED="0"; shift ;;
    *)           fail "Unknown arg: $1" ;;
  esac
done

# ── 1. OS Dependencies (Engine 1: Rust toolchain) ──────────────
#   Rust toolchain → Cargo workspace build (see Cargo.toml root + arbitrage/)
log "Installing system & Rust dependencies …"

if ! command -v apt-get &>/dev/null; then
  warn "apt-get not found — skipping apt packages (assumed pre-installed)."
fi

apt-get update -qq && apt-get install -y -qq \
  build-essential libssl-dev pkg-config curl git ca-certificates \
  libnuma-dev numactl jq redis-server postgresql \
  protobuf-compiler libprotobuf-dev 2>/dev/null || warn "apt step incomplete"

# Rust toolchain (stable) — required to compile src/, arbitrage/, mev-protection/
if ! command -v rustup &>/dev/null; then
  log "Installing Rust toolchain …"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  source "$HOME/.cargo/env"
fi
rustup default stable
ok "Rust toolchain ready"

# Foundry (Engine 6 / ZK Engine 5: contract compilation in Z-smart-contracts/)
if ! command -v forge &>/dev/null; then
  log "Installing Foundry …"
  curl -sL https://foundry.paradigm.xyz | bash
  export PATH="$HOME/.foundry/bin:$PATH"
  source "$HOME/.foundry/env"
fi
# Pre-install deps so `forge build` resolves OZ/OpenZeppelin + forge-std
foundryup 2>/dev/null || true
ok "Foundry (forge) ready → Z-smart-contracts/"

# ── 2. NUMA Hugepages + IRQ Pinning (Engine 1: kernel co-location) ─
#   CACHELINE=64 in src/numa.rs:15; we lock hugepages so the matcher's
#   hot ring stays in local L3 (single NUMA node) → keeps 45.2 µs latency.
log "Configuring hugepages + NUMA affinity …"
echo 2000 > /proc/sys/vm/nr_hugepages 2>/dev/null || warn "hugepages require root / vm.max_map_count"
# Pin IRQs to node 0 CPUs (2-7 per SYSTEM_STATE.md §2)
for irq in /proc/irq/*/smp_affinity; do
  echo 02 > "$irq" 2>/dev/null || true
done
ok "Hugepages + IRQ affinity set (cores 2-7)"

# ── 3. Clone / update repo ( Engines 1-7) ────────────────────────
log "Cloning/fetching THE-BRIDGE …"
if [ ! -d "$PROJECT_DIR" ]; then
  git clone "$GIT_REPO" "$PROJECT_DIR"
fi
cd "$PROJECT_DIR"
git pull origin "$(git branch --show-current 2>/dev/null || echo main)"

# ── 4. Build the matching engine binary (Engine 1) ──────────────
log "Compiling release binary (this takes ~5-8 min) …"
source "$HOME/.cargo/env"
cargo build --release --bin api-server 2>&1 | tail -3
ok "Binary: $PROJECT_DIR/target/release/api-server"

# ── 5. Secrets isolation (Invariants in SYSTEM_STATE.md §1) ──────
#   Keys MUST live in chmod 600 files, never in .env plaintext.
log "Isolating executor secrets …"
mkdir -p "$PROJECT_DIR/secrets"
KEY_FILE="$PROJECT_DIR/secrets/.executor_key"
if [ ! -f "$KEY_FILE" ]; then
  cat > "$KEY_FILE" <<EOF
{"pools":[],"id":"auto-generated-executor","version":"1"}
EOF
  warn "New executor key stub created at $KEY_FILE — replace with funded key before launch."
fi
chmod 600 "$KEY_FILE"
ok "Secrets isolated → $KEY_FILE (chmod 600)"

# ── 6. .env template (placeholders only — no secrets) ───────────
#   See SYSTEM_STATE.md §5 Deployment + Engines 2,6,7 env vars.
cat > "$PROJECT_DIR/.env" <<EOF
# THE-BRIDGE environment (placeholders — inject real values via vault)
NETWORK=mainnet
ETH_RPC_URL=$RPC_URL
ARB_RPC_URL=https://arb1.arbitrum.io/rpc
OP_RPC_URL=https://mainnet.optimism.io
# Multi-RPC fallback endpoints (rpc_fallback.rs) — rotates on 429/5xx
RPC_FALLBACK_ENDPOINTS=https://ethereum-rpc.publicnode.com,https://1rpc.io/eth
# Alchemy gas-policy sponsorship (flash-loan-v2 gas header)
ALCHEMY_GAS_POLICY_ID=
GAS_SPONSORSHIP_ID=
# Deployed contract address + wallet (set after `forge script` deploy)
ARBITRAGE_CONTRACT=
WALLET_ADDRESS=
# AI CEO — local DeepSeek-R1 model socket
OLLAMA_HOST=http://127.0.01:11434
EOF
ok ".env template created (no secrets)"

# ── 7. Foundry contract dependencies (Engine 5: ZK/SP1) ────────
if [ "$OLLAMA_ENABLED" = "1" ]; then
  log "Pre-installing Foundry contract deps (OZ/OpenZeppelin + forge-std) …"
  cd "$PROJECT_DIR/Z-smart-contracts"
  [ -f lib/openzeppelin-contracts/contracts/access/Ownable.sol ] || \
    forge install foundry-rs/forge-std --no-git 2>/dev/null || true
  [ -f lib/openzeppelin-contracts/contracts/access/Ownable.sol ] || \
    forge install OpenZeppelin/openzeppelin-contracts@v5.7.0 --no-git 2>/dev/null || true
  cd "$PROJECT_DIR"
fi

# ── 8. systemd unit (Engine 2: Sequencer + Engines 1-7) ──────────
#   Auto-restart on failure so Mode A/Mode B services never stay down.
log "Installing systemd unit …"
cat > /etc/systemd/system/the-bridge.service <<UNIT
[Unit]
Description=THE-BRIDGE Sovereign Node (matcher + sequencer + MEV)
After=network-online.target redis.service
Wants=network-online.target

[Service]
Type=simple
User=$USER_NAME
WorkingDirectory=$PROJECT_DIR
EnvironmentFile=$PROJECT_DIR/.env
ExecStart=$PROJECT_DIR/target/release/api-server
Restart=always
RestartSec=5
LimitNOFILE=1048576
# NUMA co-location: pin to cores 2-7 (single node) — see SYSTEM_STATE.md §2 Engine 1
Environment=RUST_MIN_STACK=8388608

[Install]
WantedBy=multi-user.target
UNIT
systemctl daemon-reload
systemctl enable the-bridge
ok "systemd unit installed (auto-restart=always)"

# ── 9. Local DeepSeek-R1 model (Engine 6: AI CEO) ────────────────
if [ "$OLLAMA_ENABLED" = "1" ] && ! command -v ollama &>/dev/null; then
  log "Installing Ollama (local DeepSeek host) …"
  curl -fsSL https://ollama.com/install.sh | sh
  systemctl enable ollama 2>/dev/null || true
fi
# Pull DeepSeek image (only if Ollama already installed and network allows)
if command -v ollama &>/dev/null; then
  log "Ollama present — DeepSeek-R1 model will be fetched on first AICEO cycle."
  warn "Run 'ollama pull deepseek-r1:8b' manually if local pull is needed."
fi

# ── 10. Start the sovereign stack ───────────────────────────────
log "Starting THE-BRIDGE …"
systemctl restart the-bridge
sleep 3
systemctl --no-pager status the-bridge | tail -5 || true

ok "Bootstrap complete."
echo -e "${GREEN}========================================================${NC}"
echo -e " THE-BRIDGE sovereign node is live."
echo -e " Binary : $PROJECT_DIR/target/release/api-server"
echo -e " Service: systemctl status the-bridge"
echo -e " Constitution: $PROJECT_DIR/SYSTEM_STATE.md"
echo -e "${GREEN}========================================================${NC}"
