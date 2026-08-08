#!/bin/bash
HOST="localhost:3001"
LOG_FILE="/var/log/the-bridge/monitor.log"

mkdir -p $(dirname "$LOG_FILE")

check() {
    local name=$1
    local url=$2
    local resp=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 "$url" 2>/dev/null)
    if [ "$resp" = "200" ]; then
        echo "[$(date)] ✅ $name — $resp"
    else
        echo "[$(date)] ❌ $name — $resp" | tee -a "$LOG_FILE"
    fi
}

echo "=========================================="
echo "THE-BRIDGE Monitor — $(date)"
echo "=========================================="

check "Health" "http://$HOST/api/v1/health"
check "Ready" "http://$HOST/ready"
check "Metrics" "http://$HOST/api/v1/metrics"

# Check memory usage
MEM=$(ps aux | grep api-server | grep -v grep | awk '{print $4}')
echo "Memory: ${MEM:-N/A}%"

# Check CPU
CPU=$(ps aux | grep api-server | grep -v grep | awk '{print $3}')
echo "CPU: ${CPU:-N/A}%"

echo ""
