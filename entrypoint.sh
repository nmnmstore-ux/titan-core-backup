#!/usr/bin/env bash
set -euo pipefail

NODE_ID="${THE_BRIDGE_NODE_ID:-engine-1}"
WAL_DIR="${THE_BRIDGE_WAL_DIR:-/var/lib/the-bridge/wal}"
HOOKS_DIR="${THE_BRIDGE_HOOKS_DIR:-/etc/the-bridge/hooks}"

mkdir -p "$WAL_DIR" "$HOOKS_DIR"

ulimit -n 1048576
ulimit -l unlimited

exec /engine
