#!/bin/bash
BRIDGE=/home/mohamednoureldinrefaay/projects/the-bridge
LOG=/var/log/the-bridge-health.log
DATE=$(date '+%Y-%m-%d %H:%M:%S')
for c in the-bridge-postgres the-bridge-redis; do
  if ! docker ps --format '{{.Names}}' | grep -q "^$c$"; then
    echo "$DATE DOWN docker $c restarting" >> $LOG
    docker start $c
  fi
done
for s in the-bridge-engine anvil; do
  if ! systemctl is-active --quiet $s; then
    echo "$DATE DOWN systemd $s restarting" >> $LOG
    sudo systemctl restart $s
  fi
done
USAGE=$(df / | awk 'NR==2 {print $5}' | tr -d %)
if [ "$USAGE" -gt 80 ]; then echo "$DATE WARN disk at ${USAGE}%" >> $LOG; fi
