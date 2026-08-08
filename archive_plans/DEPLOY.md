# THE-BRIDGE — Deployment Guide

> **Status:** Production Ready | **Last Updated:** 2026-07-27
> **Classification:** SOVEREIGN FINANCIAL INFRASTRUCTURE — RESTRICTED

---

## TABLE OF CONTENTS

1. [Hardware Requirements](#1-hardware-requirements)
2. [OS Tuning](#2-os-tuning)
3. [Environment Variables](#3-environment-variables)
4. [Deployment](#4-deployment)
5. [Monitoring](#5-monitoring)
6. [Emergency Procedures](#6-emergency-procedures)
7. [Capacity Planning](#7-capacity-planning)
8. [Operational Checklists](#8-operational-checklists)
9. [Port Mapping](#9-port-mapping)

---

## 1. Hardware Requirements

### Minimum (Development / Low Traffic)
| Component | Requirement |
|-----------|------------|
| CPU | 4 cores, x86_64, 2.5 GHz+ |
| RAM | 8 GB |
| Disk | 50 GB SSD (500+ IOPS) |
| Network | 1 Gbps |
| OS | Ubuntu 22.04+ / Debian 12+ |
| Kernel | Linux 5.15+ (for `mlock` + `schbench`) |

### Recommended (Production — 1.5M TPS)
| Component | Minimum | Recommended |
|-----------|---------|-------------|
| CPU | 16 cores, 3.5+ GHz, AVX2 | 32 cores, 4.0+ GHz, AVX-512 |
| RAM | 64 GB DDR4-3200 | 128 GB DDR5-4800 |
| NVMe | 1 TB (WAL) | 2 TB NVMe RAID 0 (WAL) + 1 TB OS |
| Network | 10 Gbps NIC | 2× 25 Gbps NIC (LACP) |
| NUMA | 2-socket | 2-socket NUMA-balanced |
| Hugepages | 2048 × 2MB pages | — |

### NUMA Topology (Recommended)
```
Socket 0: Cores 0-15  |  Socket 1: Cores 16-31
  Memory: 64GB           Memory: 64GB
  NIC: eth0 (queue 0-7)  NIC: eth1 (queue 8-15)
```

### Verification Script
```bash
# Check NUMA
numactl --hardware

# Check hugepages
grep HugePages_Total /proc/meminfo

# Check governor
cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor

# Check SMT
cat /sys/devices/system/cpu/smt/active

# Disk IOPS (fio)
fio --randrepeat=1 --ioengine=libaio --direct=1 --gtod_reduce=1 --name=test \
    --bs=4k --iodepth=64 --size=1G --readwrite=randrw --rwmixread=50
```

---

## 2. OS Tuning

### 2.1 CPU Governor
```bash
cpupower frequency-set -g performance
echo performance | tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
```

### 2.2 Disable Hyperthreading
```bash
echo off > /sys/devices/system/cpu/smt/control
# Persistent: add 'nosmt' to kernel cmdline
```

### 2.3 Hugepages
```bash
echo 2048 > /proc/sys/vm/nr_hugepages
# Persistent:
echo 'vm.nr_hugepages = 2048' > /etc/sysctl.d/99-the-bridge.conf
```

### 2.4 Network Stack Tuning
```bash
cat > /etc/sysctl.d/99-the-bridge-network.conf <<'EOF'
net.core.rmem_max = 134217728
net.core.wmem_max = 134217728
net.core.rmem_default = 16777216
net.core.wmem_default = 16777216
net.core.netdev_budget = 600
net.core.netdev_budget_usecs = 8000
net.core.somaxconn = 65535
net.ipv4.tcp_max_syn_backlog = 65535
net.ipv4.tcp_fastopen = 3
net.ipv4.tcp_tw_reuse = 1
net.ipv4.tcp_fin_timeout = 10
net.core.busy_poll = 50
net.core.busy_read = 50
net.ipv4.tcp_notsent_lowat = 16384
net.core.default_qdisc = fq
net.ipv4.tcp_congestion_control = bbr
EOF
sysctl --system
```

### 2.5 IRQ Affinity (10Gbps NIC)
```bash
# Pin NIC queues to dedicated cores
for i in {0..7}; do
  echo $i > /proc/irq/$(grep "eth0-TxRx-$i" /proc/interrupts | awk '{print $1}' | tr -d ':')/smp_affinity_list
done

# Persistent via systemd service
systemctl disable irqbalance
cat > /etc/systemd/system/irq-affinity.service <<'EOF'
[Unit]
Description=IRQ Affinity for THE-BRIDGE
After=network.target

[Service]
Type=oneshot
ExecStart=/usr/local/sbin/set-irq-affinity.sh
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
EOF
systemctl enable irq-affinity
```

### 2.6 Kernel Parameters
```bash
cat > /etc/sysctl.d/99-the-bridge.conf <<'EOF'
# Memory
vm.swappiness = 1
vm.dirty_ratio = 10
vm.dirty_background_ratio = 5
vm.zone_reclaim_mode = 0

# Network
net.core.rmem_max = 134217728
net.core.wmem_max = 134217728
net.core.netdev_budget = 600
net.core.netdev_budget_usecs = 8000
net.core.somaxconn = 65535
net.ipv4.tcp_max_syn_backlog = 65535
net.ipv4.tcp_fastopen = 3
net.ipv4.tcp_tw_reuse = 1
net.ipv4.tcp_fin_timeout = 10
net.core.busy_poll = 50
net.core.busy_read = 50
net.ipv4.tcp_notsent_lowat = 16384
net.core.default_qdisc = fq
net.ipv4.tcp_congestion_control = bbr

# FS
fs.file-max = 2000000
fs.nr_open = 2000000
EOF
```

---

## 3. Environment Variables

All variables prefixed with `THE_BRIDGE_`.

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `THE_BRIDGE_NODE_ID` | Yes | `engine-1` | Unique node identifier |
| `THE_BRIDGE_PEERS` | No | `` | DAG consensus peers (comma-separated host:port) |
| `THE_BRIDGE_REPLICAS` | No | `` | WAL sync replicas (comma-separated host:port) |
| `THE_BRIDGE_WAL_DIR` | No | `/var/lib/the-bridge/wal` | WAL directory (must be on NVMe) |
| `THE_BRIDGE_HOOKS_DIR` | No | `/etc/the-bridge/hooks` | WASM hooks directory |
| `THE_BRIDGE_JWT_SECRET` | **Yes** | — | 64-byte base64 secret for JWT signing |
| `THE_BRIDGE_REGULATOR_SECRET` | **Yes** | — | 64-byte base64 regulator signing key |
| `THE_BRIDGE_CORS_ORIGINS` | No | `*` | Comma-separated allowed origins |
| `THE_BRIDGE_TLS_CERT` | Prod | — | Path to TLS certificate (PEM) |
| `THE_BRIDGE_TLS_KEY` | Prod | — | Path to TLS private key (PEM) |
| `THE_BRIDGE_DATABASE_URL` | Prod | — | PostgreSQL DSN |
| `THE_BRIDGE_API_PORT` | No | `3001` | REST API port |
| `THE_BRIDGE_FIX_PORT` | No | `4001` | FIX gateway port |
| `THE_BRIDGE_CONSENSUS_PORT` | No | `4002` | DAG consensus gossip port |
| `THE_BRIDGE_METRICS_PORT` | No | `9090` | Prometheus metrics port |
| `THE_BRIDGE_LOG_LEVEL` | No | `info` | `trace|debug|info|warn|error` |
| `THE_BRIDGE_WAL_SYNC` | No | `true` | Sync WAL on every write (fsync) |
| `THE_BRIDGE_WAL_BATCH_SIZE` | No | `64` | Max WAL batch size |
| `THE_BRIDGE_MAX_ORDERS_PER_SECOND` | No | `1500000` | Rate limit per node |

### Generate Secrets
```bash
openssl rand -base64 64  # THE_BRIDGE_JWT_SECRET
openssl rand -base64 64  # THE_BRIDGE_REGULATOR_SECRET
```

### Example .env.prod
```bash
THE_BRIDGE_NODE_ID=bridge-prod-1
THE_BRIDGE_PEERS=bridge-prod-2:4002,bridge-prod-3:4002
THE_BRIDGE_REPLICAS=wal-replica-1:4001,wal-replica-2:4001,wal-replica-3:4001
THE_BRIDGE_WAL_DIR=/mnt/nvme/wal
THE_BRIDGE_HOOKS_DIR=/etc/the-bridge/hooks
THE_BRIDGE_JWT_SECRET=<64-byte-base64>
THE_BRIDGE_REGULATOR_SECRET=<64-byte-base64>
THE_BRIDGE_CORS_ORIGINS=https://trade.bank.com,https://api.broker.com
THE_BRIDGE_TLS_CERT=/etc/ssl/the-bridge/cert.pem
THE_BRIDGE_TLS_KEY=/etc/ssl/the-bridge/key.pem
THE_BRIDGE_DATABASE_URL=postgres://bridge:pass@db.prod:5432/the_bridge
THE_BRIDGE_WAL_SYNC=true
THE_BRIDGE_WAL_BATCH_SIZE=64
RUST_LOG=the_bridge=info,tower_http=warn
RUST_BACKTRACE=1
```

---

## 4. Deployment

### 4.1 Systemd Unit
```ini
# /etc/systemd/system/the-bridge.service
[Unit]
Description=THE-BRIDGE Matching Engine
After=network-online.target
Wants=network-online.target
After=postgresql.service
Wants=postgresql.service

[Service]
Type=notify
User=the-bridge
Group=the-bridge
WorkingDirectory=/opt/the-bridge
EnvironmentFile=/opt/the-bridge/.env.prod
ExecStart=/opt/the-bridge/the-bridge
ExecReload=/bin/kill -HUP $MAINPID
KillMode=mixed
KillSignal=SIGTERM
TimeoutStopSec=30
TimeoutStartSec=60
Restart=on-failure
RestartSec=5
LimitNOFILE=2000000
LimitMEMLOCK=infinity
AmbientCapabilities=CAP_NET_BIND_SERVICE CAP_IPC_LOCK CAP_SYS_RESOURCE
Nice=-10
CPUAffinity=0-15
MemoryLimit=64G

# Security
NoNewPrivileges=yes
PrivateTmp=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/mnt/nvme/wal /etc/the-bridge/hooks /var/log/the-bridge
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectControlGroups=yes
RestrictRealtime=yes
RestrictNamespaces=yes
LockPersonality=yes
MemoryDenyWriteExecute=yes
SystemCallFilter=@system-service @clock @cpu-emulation @debug @module @network-io @obsolete @privileged @raw-io @reboot @resources @setuid

[Install]
WantedBy=multi-user.target
```

### 4.2 Docker Compose
```yaml
# docker-compose.prod.yml
version: '3.8'

services:
  the-bridge:
    image: the-bridge:latest
    container_name: the-bridge
    restart: unless-stopped
    user: "1000:1000"
    cpuset: "0-15"
    mem_limit: 64g
    memlock: -1
    ulimits:
      nofile: 2000000
      memlock: -1
    environment:
      - THE_BRIDGE_NODE_ID=bridge-prod-1
      - THE_BRIDGE_PEERS=bridge-prod-2:4002,bridge-prod-3:4002
      - THE_BRIDGE_REPLICAS=wal-replica-1:4001,wal-replica-2:4001,wal-replica-3:4001
      - THE_BRIDGE_WAL_DIR=/data/wal
      - THE_BRIDGE_HOOKS_DIR=/hooks
      - THE_BRIDGE_JWT_SECRET=${JWT_SECRET}
      - THE_BRIDGE_REGULATOR_SECRET=${REGULATOR_SECRET}
      - THE_BRIDGE_CORS_ORIGINS=https://trade.bank.com,https://api.broker.com
      - THE_BRIDGE_TLS_CERT=/certs/cert.pem
      - THE_BRIDGE_TLS_KEY=/certs/key.pem
      - THE_BRIDGE_DATABASE_URL=postgres://bridge:${DB_PASS}@db:5432/the_bridge
      - THE_BRIDGE_WAL_SYNC=true
      - THE_BRIDGE_WAL_BATCH_SIZE=64
      - RUST_LOG=the_bridge=info
      - RUST_BACKTRACE=1
    volumes:
      - wal-data:/data/wal
      - ./hooks:/hooks:ro
      - ./certs:/certs:ro
      - ./logs:/var/log/the-bridge
    ports:
      - "3001:3001"   # REST API
      - "4001:4001"   # FIX Gateway
      - "4002:4002"   # Consensus
      - "9090:9090"   # Prometheus
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3001/api/v1/health"]
      interval: 10s
      timeout: 3s
      retries: 3
      start_period: 30s
    deploy:
      resources:
        reservations:
          cpus: '16'
          memory: 64G
        limits:
          cpus: '16'
          memory: 64G

  postgres:
    image: postgres:16
    container_name: the-bridge-db
    restart: unless-stopped
    environment:
      POSTGRES_DB: the_bridge
      POSTGRES_USER: bridge
      POSTGRES_PASSWORD: ${DB_PASS}
    volumes:
      - pg-data:/var/lib/postgresql/data
    ports:
      - "5432:5432"
    command: >
      postgres
      -c max_connections=500
      -c shared_buffers=16GB
      -c effective_cache_size=48GB
      -c work_mem=256MB
      -c maintenance_work_mem=2GB
      -c max_wal_size=4GB
      -c min_wal_size=1GB
      -c checkpoint_completion_target=0.9
      -c wal_buffers=64MB
      -c default_statistics_target=100
      -c random_page_cost=1.1
      -c effective_io_concurrency=200

  prometheus:
    image: prom/prometheus:v2.47
    container_name: the-bridge-prometheus
    restart: unless-stopped
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml:ro
      - prom-data:/prometheus
    ports:
      - "9091:9090"
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
      - '--storage.tsdb.retention.time=30d'

  grafana:
    image: grafana/grafana:10.1
    container_name: the-bridge-grafana
    restart: unless-stopped
    environment:
      GF_SECURITY_ADMIN_PASSWORD: ${GRAFANA_PASS}
      GF_USERS_ALLOW_SIGN_UP: "false"
    volumes:
      - ./grafana/dashboards:/etc/grafana/provisioning/dashboards:ro
      - ./grafana/datasources:/etc/grafana/provisioning/datasources:ro
      - grafana-data:/var/lib/grafana
    ports:
      - "3000:3000"

volumes:
  wal-data:
  pg-data:
  prom-data:
  grafana-data:

networks:
  default:
    driver: bridge
    ipam:
      config:
        - subnet: 172.28.0.0/16
```

### 4.3 Health Checks

| Endpoint | Method | Expected | SLA |
|----------|--------|----------|-----|
| `/api/v1/health` | GET | `200 OK` `{"status":"healthy"}` | < 5ms |
| `/api/v1/ready` | GET | `200 OK` `{"ready":true}` | < 5ms |
| `/api/v1/metrics` | GET | `200 OK` Prometheus format | < 10ms |

---

## 5. Monitoring

### 5.1 Prometheus Config
```yaml
global:
  scrape_interval: 5s
  evaluation_interval: 5s
  external_labels:
    cluster: 'the-bridge-prod'
    env: 'production'

scrape_configs:
  - job_name: 'the-bridge'
    static_configs:
      - targets: ['the-bridge:9090']
    metrics_path: '/api/v1/metrics'

  - job_name: 'node-exporter'
    static_configs:
      - targets: ['node-exporter:9100']

  - job_name: 'postgres'
    static_configs:
      - targets: ['postgres-exporter:9187']

alerting:
  alertmanagers:
    - static_configs:
        - targets: ['alertmanager:9093']

rule_files:
  - '/etc/prometheus/rules/*.yml'
```

### 5.2 Alert Rules
```yaml
groups:
  - name: the-bridge
    interval: 10s
    rules:
      - alert: TheBridgeHighLatency
        expr: histogram_quantile(0.99, rate(the_bridge_order_latency_seconds_bucket[1m])) > 0.000035
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "P99 latency > 35µs"

      - alert: TheBridgeLowThroughput
        expr: rate(the_bridge_orders_total[1m]) < 100000
        for: 2m
        labels:
          severity: warning
        annotations:
          summary: "TPS < 100K"

      - alert: TheBridgeWALLag
        expr: the_bridge_wal_replication_lag > 1000
        for: 30s
        labels:
          severity: critical
        annotations:
          summary: "WAL replication lag > 1000"

      - alert: TheBridgeHighErrorRate
        expr: rate(the_bridge_errors_total[5m]) > 0.01
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Error rate > 1%"

      - alert: TheBridgeKillSwitchArmed
        expr: the_bridge_kill_switch_armed == 1
        for: 0s
        labels:
          severity: critical
        annotations:
          summary: "SOVEREIGN KILL SWITCH ARMED"

      - alert: TheBridgeKillSwitchActivated
        expr: the_bridge_kill_switch_active == 1
        for: 0s
        labels:
          severity: critical
        annotations:
          summary: "SOVEREIGN KILL SWITCH ACTIVATED"

      - alert: TheBridgeMemoryHigh
        expr: (container_memory_usage_bytes{name="the-bridge"} / container_spec_memory_limit_bytes{name="the-bridge"}) > 0.85
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Memory usage > 85%"

      - alert: TheBridgeCPUSaturation
        expr: rate(container_cpu_usage_seconds_total{name="the-bridge"}[1m]) > 15
        for: 2m
        labels:
          severity: warning
        annotations:
          summary: "CPU usage > 15 cores sustained"

      - alert: TheBridgeWALDiskSpace
        expr: (node_filesystem_avail_bytes{mountpoint="/mnt/nvme"} / node_filesystem_size_bytes{mountpoint="/mnt/nvme"}) < 0.15
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "WAL disk < 15% free"
```

### 5.3 Grafana Dashboards

Import from `deploy/grafana/dashboards/`:
- `the-bridge-overview.json` — TPS, latency, error rate, queue depth
- `the-bridge-matching.json` — Order book depth, match latency, trade volume
- `the-bridge-consensus.json` — DAG height, peer count, commit latency
- `the-bridge-wal.json` — WAL write latency, replication lag, disk usage
- `the-bridge-cloak.json` — Threat score, kill switch state, migration status
- `the-bridge-system.json` — CPU, memory, network, disk, NUMA stats

---

## 6. Emergency Procedures

### 6.1 Sovereign Kill Switch
```bash
# Activate (shields engine, rejects new orders, drains book)
curl -X POST http://localhost:3001/api/v1/sovereign/shield \
  -H "Authorization: Bearer $REGULATOR_TOKEN"

# Deactivate (requires regulator signature)
curl -X POST http://localhost:3001/api/v1/sovereign/stand-down \
  -H "Authorization: Bearer $REGULATOR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"signature":"<ed25519_regulator_sig>"}'

# Status
curl http://localhost:3001/api/v1/sovereign/status
```

**Kill Switch States:**
- `0` = Disarmed (normal)
- `1` = Armed (threat detected, evaluating)
- `2` = Active (shielded, draining book)
- `3` = Stand-down pending (awaiting regulator sig)

### 6.2 WAL Recovery
```bash
# Auto-recovery on startup
curl http://localhost:3001/api/v1/wal/status

# Force recovery from specific LSN
curl -X POST http://localhost:3001/api/v1/wal/recover \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -d '{"from_lsn": "0x2B8000"}'

# Verify integrity
curl -X POST http://localhost:3001/api/v1/wal/verify \
  -H "Authorization: Bearer $ADMIN_TOKEN"
```

### 6.3 Full Node Reset (Nuclear Option)
```bash
# 1. Stop service
systemctl stop the-bridge

# 2. Verify no active consensus peers depending on this node
curl http://peer2:3001/api/v1/consensus/peers | jq '.[] | select(.id=="bridge-prod-1")'

# 3. Wipe WAL
rm -rf /mnt/nvme/wal/*

# 4. Reset database
psql $DATABASE_URL -c "TRUNCATE trades, settlements, positions RESTART IDENTITY;"

# 5. Restart
systemctl start the-bridge

# 6. Verify sync
watch -n 2 'curl -s localhost:3001/api/v1/consensus/status | jq .height'
```

### 6.4 Hot Migration (Zero-Downtime Failover)
```bash
# 1. Prepare target node (same config, different NODE_ID)
# 2. Start target, verify healthy
# 3. Initiate migration from source
curl -X POST http://source:3001/api/v1/sovereign/migrate \
  -H "Authorization: Bearer $REGULATOR_TOKEN" \
  -d '{"target_node":"bridge-prod-2","timeout_ms":5000}'

# 4. Monitor migration
watch -n 1 'curl -s source:3001/api/v1/sovereign/migration/status | jq .'

# 5. Cutover DNS / load balancer to target
# 6. Decommission source after verification
```

---

## 7. Capacity Planning

| Metric | Target | Capacity per Node | Scaling |
|--------|--------|-------------------|---------|
| **Peak TPS** | 1.5M | 1.5M @ 16 cores | Horizontal (DAG shards) |
| **P99 Latency** | < 35µs | — | Vertical (CPU freq, NIC) |
| **Order Book Depth** | 10M orders | ~3 GB RAM | Vertical (RAM) |
| **WAL Write Rate** | 2M ops/sec | 100 GB/day peak | NVMe RAID 0 |
| **WAL Retention** | 7 days | 700 GB | Tier to S3 after 24h |
| **Consensus Peers** | 3-7 | 3 recommended | Odd numbers only |
| **WAL Replicas** | 3 | 3 sync replicas | Quorum: 2/3 |
| **FIX Sessions** | 500/session | 50 sessions/node | Horizontal |
| **REST Connections** | 10K concurrent | 10K / node | Horizontal |

### Resource Formulas

| Resource | Formula |
|----------|---------|
| **RAM (GB)** | `3 + (max_orders / 1_000_000) * 0.3 + (wal_buffer_mb / 1024)` |
| **WAL Disk (GB/day)** | `peak_tps * avg_record_bytes * 86400 / 1e9` ≈ `1.5M * 800 * 86400 / 1e9` ≈ **104 GB/day** |
| **CPU Cores** | `max(tps / 100000, 16)` dedicated cores |
| **Network (Gbps)** | `tps * (avg_order_bytes + avg_trade_bytes) * 8 / 1e9` ≈ **12 Gbps @ 1.5M TPS** |

### Scaling Triggers

| Trigger | Action |
|---------|--------|
| P99 > 30µs sustained 5m | Scale up CPU freq / add core |
| TPS > 1.2M sustained | Add DAG shard / new node |
| WAL lag > 500 | Add replica / faster NVMe |
| Memory > 80% | Increase RAM / reduce book depth |
| Consensus height lag > 100 | Check peer health / network |

---

## 8. Operational Checklists

### 8.1 Pre-Deployment
- [ ] Hardware matches spec (CPU, RAM, NVMe, NIC)
- [ ] OS tuned (cpupower, hugepages, sysctl, IRQ affinity)
- [ ] Secrets generated and stored in vault
- [ ] TLS certs valid (> 30 days)
- [ ] PostgreSQL tuned (shared_buffers, work_mem, wal_buffers)
- [ ] Prometheus + Grafana deployed
- [ ] Alertmanager configured with PagerDuty/Slack
- [ ] Runbooks printed and accessible offline

### 8.2 Go-Live
- [ ] Deploy to staging, run integration tests
- [ ] Load test at 1.5M TPS for 30 min
- [ ] Verify kill switch activation/deactivation
- [ ] Verify WAL recovery from crash
- [ ] Verify hot migration
- [ ] Cutover DNS / LB
- [ ] Monitor for 30 min at full load

### 8.3 Daily Operations
- [ ] Check health endpoints (06:00, 14:00, 22:00)
- [ ] Review WAL disk usage
- [ ] Verify replica sync status
- [ ] Check threat analyzer score
- [ ] Review alert history

### 8.4 Weekly
- [ ] WAL integrity verification
- [ ] Certificate expiry check
- [ ] Capacity review (TPS trend, disk growth)
- [ ] DR drill (kill switch test on staging)

---

## 9. Port Mapping

| Port | Protocol | Service |
|------|----------|---------|
| 80 | TCP | HTTP → Caddy (redirect to 443) |
| 443 | TCP | HTTPS → Caddy → Engine |
| 3001 | TCP | Engine REST API (behind Caddy) |
| 4001 | TCP | FIX 5.0 SP2 Gateway |
| 4002 | TCP | DAG Consensus Gossip |
| 9090 | TCP | Prometheus (internal) |
| 3000 | TCP | Grafana (internal — admin) |

---

## 10. Key Endpoints Reference

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/api/v1/order` | POST | JWT | Place order |
| `/api/v1/order/{id}` | DELETE | JWT | Cancel order |
| `/api/v1/orders` | GET | JWT | List open orders |
| `/api/v1/trades` | GET | JWT | Trade history |
| `/api/v1/book/{symbol}` | GET | None | Order book snapshot |
| `/api/v1/health` | GET | None | Liveness |
| `/api/v1/ready` | GET | None | Readiness |
| `/api/v1/metrics` | GET | None | Prometheus |
| `/api/v1/wal/status` | GET | Admin | WAL status |
| `/api/v1/wal/recover` | POST | Admin | Force WAL recovery |
| `/api/v1/consensus/status` | GET | None | DAG status |
| `/api/v1/consensus/peers` | GET | None | Peer list |
| `/api/v1/sovereign/status` | GET | None | Kill switch status |
| `/api/v1/sovereign/shield` | POST | Regulator | Activate kill switch |
| `/api/v1/sovereign/stand-down` | POST | Regulator | Deactivate kill switch |
| `/api/v1/sovereign/migrate` | POST | Regulator | Hot migrate |
| `/api/v1/hooks/list` | GET | Admin | List WASM hooks |
| `/api/v1/hooks/reload` | POST | Admin | Reload hooks |

---

**CLASSIFICATION: SOVEREIGN FINANCIAL INFRASTRUCTURE — RESTRICTED**
