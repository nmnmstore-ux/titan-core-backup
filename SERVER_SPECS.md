# THE-BRIDGE — Server Specifications

## Minimum (Development / Low Traffic)
| Component | Requirement |
|-----------|------------|
| CPU | 4 cores, x86_64, 2.5 GHz+ |
| RAM | 8 GB |
| Disk | 50 GB SSD (500+ IOPS) |
| Network | 1 Gbps |
| OS | Ubuntu 22.04+ / Debian 12+ |
| Kernel | Linux 5.15+ (for `mlock` + `schbench`) |

## Recommended (Production — 1.5M TPS target)
| Component | Requirement |
|-----------|------------|
| CPU | 16+ cores, **NUMA-aware** (AMD EPYC / Intel Xeon), 3.0 GHz+ |
| RAM | 64 GB+ (DDR5 preferred) |
| Disk | 500 GB NVMe (50k+ random IOPS) |
| Network | 25 Gbps+ (dual NIC for redundancy) |
| OS | Ubuntu 24.04 LTS / Debian 12 |
| Kernel | Linux 6.2+ with `CONFIG_NUMA`, `CONFIG_HUGETLB` |
| Hugepages | 2048 × 2MB pages (`echo 2048 > /proc/sys/vm/nr_hugepages`) |

## Performance Tuning Checklist
```bash
cpupower frequency-set -g performance
echo performance > /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
echo off > /sys/devices/system/cpu/smt/control          # disable hyperthreading
echo 2048 > /proc/sys/vm/nr_hugepages                    # 2MB hugepages
sysctl -w net.core.rmem_max=134217728
sysctl -w net.core.wmem_max=134217728
sysctl -w net.core.netdev_budget=600
sysctl -w vm.swappiness=1
```

## Ports
| Port | Protocol | Purpose |
|------|----------|---------|
| 80   | TCP      | HTTP → Caddy (redirect to 443) |
| 443  | TCP      | HTTPS → Caddy → Engine |
| 3001 | TCP      | Engine REST API (internal — behind Caddy) |
| 4001 | TCP      | FIX 5.0 SP2 Gateway |
| 4002 | TCP      | DAG Consensus Gossip |
| 9090 | TCP      | Prometheus (internal) |
| 3000 | TCP      | Grafana (internal — admin) |

## Verification Script
Run after provisioning:
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
