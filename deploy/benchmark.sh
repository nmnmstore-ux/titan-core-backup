#!/bin/bash
HOST="localhost:3001"
echo "THE-BRIDGE Benchmark — $(date)"
echo "=========================================="

# Place N orders and measure throughput
N=1000
echo "Placing $N orders..."

START=$(date +%s%N)
for i in $(seq 1 $N); do
    curl -s -X POST "http://$HOST/api/v1/order" \
        -H "Content-Type: application/json" \
        -d "{\"pair\":\"BTC/USD\",\"side\":\"Buy\",\"order_type\":\"Limit\",\"price\":50000.0,\"quantity\":0.001}" \
        -o /dev/null &
done
wait
END=$(date +%s%N)

DURATION_MS=$(( (END - START) / 1000000 ))
TPS=$(( N * 1000 / DURATION_MS ))

echo "Time: ${DURATION_MS}ms"
echo "TPS: ~${TPS}"
echo "=========================================="
