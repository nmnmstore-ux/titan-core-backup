#!/usr/bin/env bash
# THE-BRIDGE :: decrypt-secrets.sh
# Decrypts .secrets.env.enc to stdout using the master key.
# Usage: ./decrypt-secrets.sh            # reads key from $MASTER_KEY or prompts
#        MASTER_KEY=... ./decrypt-secrets.sh
set -euo pipefail
cd "$(dirname "$0")"

if [[ -z "${MASTER_KEY:-}" ]]; then
  KEY_SOURCE=/tmp/the-bridge-master.key
  if [[ -f "$KEY_SOURCE" ]]; then
    MASTER_KEY="$(cat "$KEY_SOURCE")"
  else
    read -rsp "Enter master key: " MASTER_KEY; echo
  fi
fi

openssl enc -d -aes-256-cbc -pbkdf2 -iter 100000 \
  -in .secrets.env.enc -pass "pass:$MASTER_KEY"