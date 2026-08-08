#!/bin/bash
export THE_BRIDGE_NODE_ID="engine-1"
export RUST_LOG="the_bridge=info"
export RUST_BACKTRACE=1

# Kill any existing instance
pkill -f api-server 2>/dev/null || true
sleep 1

# Start the server
cd /home/mohamednoureldinrefaay/projects/the-bridge
./target/release/api-server &
SERVER_PID=$!
echo "Server started with PID: $SERVER_PID"

# Wait for it to be ready
sleep 3

# Test health endpoint
curl -s http://localhost:3001/api/v1/health 2>/dev/null || echo "Health check failed"
curl -s http://localhost:3001/ready 2>/dev/null || echo "Ready check failed"

echo "Server is running on port 3001"
