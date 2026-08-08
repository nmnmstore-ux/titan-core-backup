#!/usr/bin/env python3
"""THE-BRIDGE HTTP Performance Benchmark Runner"""

import json
import time
import statistics
import sys
import os
import threading
import urllib.request
import urllib.error
from concurrent.futures import ThreadPoolExecutor, as_completed

API_BASE = os.environ.get("API_BASE", "http://localhost:3001")
API_KEY = os.environ.get("API_KEY", "tb_2c55e600_1a1c1815cc0e81e91ea4118d322c06f22157951122588d9ea63652f3b92aff7a")
USER_ID = os.environ.get("USER_ID", "4746fc2f-c44c-43b1-9d30-ca75a911c419")
PAIRS = ["EUR/USD", "GBP/USD", "USD/JPY", "BTC/USD", "ETH/USD", "SOL/USD"]

def make_order_body(pair, side, price, qty):
    side_val = 1 if side == "Sell" else 0
    return json.dumps({
        "id": "00000000-0000-0000-0000-000000000000",
        "user_id": USER_ID,
        "pair": pair,
        "order_type": 1,  # Limit
        "filled_quantity": 0,
        "side": side_val,  # 0=Buy, 1=Sell
        "price": price,
        "quantity": qty,
        "filled": 0.0,
        "remaining": qty,
        "status": 0,  # New
        "timestamp": 0,
        "ttl_ms": None,
        "is_swap": False,
        "swap_target_currency": None,
        "tee_signed": False,
        "dot_verified": False,
        "stealth": False,
        "trailing_offset": None,
        "trigger_price": None,
        "hard_floor": None,
        "track": 0,  # Compliant
        "style": "Standard",
        "hidden_remaining": 0.0,
        "client_order_id": None
    }).encode()

def post_order(body):
    req = urllib.request.Request(
        f"{API_BASE}/api/v1/order",
        data=body,
        headers={
            "Authorization": f"Bearer {API_KEY}",
            "Content-Type": "application/json",
        },
        method="POST"
    )
    t0 = time.perf_counter()
    try:
        resp = urllib.request.urlopen(req, timeout=30)
        data = resp.read()
        elapsed = time.perf_counter() - t0
        return (json.loads(data), elapsed)
    except Exception as e:
        elapsed = time.perf_counter() - t0
        return ({"error": str(e)}, elapsed)

def percentile(data, p):
    if not data:
        return 0
    sorted_data = sorted(data)
    idx = int(len(sorted_data) * p / 100)
    return sorted_data[min(idx, len(sorted_data) - 1)]

def print_latencies(name, latencies_us):
    if not latencies_us:
        return
    latencies_us.sort()
    p50 = percentile(latencies_us, 50)
    p90 = percentile(latencies_us, 90)
    p99 = percentile(latencies_us, 99)
    p999 = percentile(latencies_us, 99.9)
    avg = statistics.mean(latencies_us)
    mn = latencies_us[0]
    mx = latencies_us[-1]
    print(f"  {name:<35} min={mn:>8}us  avg={avg:>8.0f}us  P50={p50:>8}us  P90={p90:>8}us  P99={p99:>8}us  P999={p999:>9}us  max={mx:>8}us")

def print_header(title):
    print(f"\n{'='*90}")
    print(f"  {title}")
    print(f"{'='*90}")

def benchmark_sequential():
    print_header("BENCHMARK 1: Sequential Order Placement (10,000 limit orders via HTTP)")
    pair = PAIRS[0]
    n = 10000
    latencies = []

    # Warmup
    body = make_order_body(pair, "Buy", 1.0, 0.01)
    for _ in range(20):
        post_order(body)

    start = time.perf_counter()
    for i in range(n):
        price = 0.5 + (i * 0.0001)
        body = make_order_body(pair, "Buy", price, 0.01)
        result, elapsed = post_order(body)
        if "error" not in result:
            latencies.append(elapsed * 1_000_000)
        elif i % 100 == 0:
            print(f"  [WARN] order {i} failed: {result.get('error')}")

    elapsed = time.perf_counter() - start
    success = len(latencies)
    print_latencies("place_order (HTTP)", latencies)
    print(f"  {'Result:':<35} {success}/{n} orders successful")
    print(f"  {'Total time:':<35} {elapsed:.2f}s")
    if elapsed > 0:
        print(f"  {'Throughput:':<35} {success/elapsed:.0f} orders/sec")

def benchmark_concurrent():
    print_header("BENCHMARK 2: Concurrent Order Placement (10,000 orders, 100 concurrent)")
    pair = PAIRS[0]
    n = 10000
    concurrency = 100
    success = [0]
    errors = [0]
    lock = threading.Lock()

    def worker(thread_id):
        client_n = n // concurrency
        for i in range(client_n):
            price = 0.5 + ((thread_id * client_n + i) * 0.0001)
            body = make_order_body(pair, "Buy", price, 0.01)
            try:
                req = urllib.request.Request(
                    f"{API_BASE}/api/v1/order",
                    data=body,
                    headers={
                        "Authorization": f"Bearer {API_KEY}",
                        "Content-Type": "application/json",
                    },
                    method="POST"
                )
                resp = urllib.request.urlopen(req, timeout=30)
                with lock:
                    success[0] += 1
            except Exception:
                with lock:
                    errors[0] += 1

    start = time.perf_counter()
    threads = []
    for tid in range(concurrency):
        t = threading.Thread(target=worker, args=(tid,))
        threads.append(t)
        t.start()
    for t in threads:
        t.join()
    elapsed = time.perf_counter() - start

    print(f"  {'Orders placed:':<35} {success[0]}")
    print(f"  {'Errors:':<35} {errors[0]}")
    print(f"  {'Total time:':<35} {elapsed:.2f}s")
    if elapsed > 0:
        print(f"  {'Throughput:':<35} {success[0]/elapsed:.0f} orders/sec")

def benchmark_multi_pair():
    print_header("BENCHMARK 3: Multi-Pair Throughput (orders across 5 pairs)")
    pairs = PAIRS[:5]
    orders_per_pair = 2000
    n = orders_per_pair * len(pairs)
    success = 0

    start = time.perf_counter()
    for pair in pairs:
        for i in range(orders_per_pair):
            side = "Buy" if i % 2 == 0 else "Sell"
            price = 0.5 + (i * 0.001) if side == "Buy" else 1.5 + (i * 0.001)
            body = make_order_body(pair, side, price, 0.01)
            result, _ = post_order(body)
            if "error" not in result:
                success += 1
            elif i % 500 == 0:
                print(f"  [WARN] order {i} on {pair} failed")
    elapsed = time.perf_counter() - start

    print(f"  {'Result:':<35} {success}/{n} orders placed")
    print(f"  {'Total time:':<35} {elapsed:.2f}s")
    if elapsed > 0:
        print(f"  {'Throughput:':<35} {success/elapsed:.0f} orders/sec")

def benchmark_full_pipeline():
    print_header("BENCHMARK 4: Full Pipeline Latency (5,000 HTTP POST -> response)")
    pair = PAIRS[0]
    n = 5000
    latencies = []

    start = time.perf_counter()
    for i in range(n):
        price = 0.5 + (i * 0.0001)
        body = make_order_body(pair, "Buy", price, 0.01)
        result, elapsed = post_order(body)
        if "error" not in result:
            latencies.append(elapsed * 1_000_000)
        elif i % 100 == 0:
            print(f"  [WARN] order {i} failed: {result.get('error')}")
    elapsed = time.perf_counter() - start
    success = len(latencies)

    print_latencies("full pipeline (HTTP)", latencies)
    print(f"  {'Result:':<35} {success}/{n} successful")
    print(f"  {'Total time:':<35} {elapsed:.2f}s")
    if elapsed > 0:
        print(f"  {'Throughput:':<35} {success/elapsed:.0f} req/sec")

def benchmark_concurrent_clients():
    print_header("BENCHMARK 5: Concurrent Clients (10 clients x 1000 orders each)")
    num_clients = 10
    orders_per_client = 1000
    n = num_clients * orders_per_client
    success = [0]
    errors = [0]
    lock = threading.Lock()

    def client_worker(client_id):
        for i in range(orders_per_client):
            price = 0.5 + ((client_id * orders_per_client + i) * 0.001)
            body = make_order_body(PAIRS[0], "Buy", price, 0.01)
            try:
                req = urllib.request.Request(
                    f"{API_BASE}/api/v1/order",
                    data=body,
                    headers={
                        "Authorization": f"Bearer {API_KEY}",
                        "Content-Type": "application/json",
                    },
                    method="POST"
                )
                resp = urllib.request.urlopen(req, timeout=30)
                with lock:
                    success[0] += 1
            except Exception:
                with lock:
                    errors[0] += 1

    start = time.perf_counter()
    threads = []
    for cid in range(num_clients):
        t = threading.Thread(target=client_worker, args=(cid,))
        threads.append(t)
        t.start()
    for t in threads:
        t.join()
    elapsed = time.perf_counter() - start
    error_rate = errors[0] / n * 100 if n > 0 else 0

    print(f"  {'Orders completed:':<35} {success[0]}/{n}")
    print(f"  {'Errors:':<35} {errors[0]} ({error_rate:.2f}%)")
    print(f"  {'Total time:':<35} {elapsed:.2f}s")
    if elapsed > 0:
        print(f"  {'Throughput:':<35} {success[0]/elapsed:.0f} orders/sec")

def check_server():
    try:
        req = urllib.request.Request(f"{API_BASE}/api/v1/health")
        resp = urllib.request.urlopen(req, timeout=5)
        data = json.loads(resp.read())
        print(f"  Server status: {data.get('status', 'unknown')}")
        print(f"  Uptime: {data.get('uptime', 'N/A')}")
        print(f"  Version: {data.get('version', 'N/A')}")
        return True
    except Exception as e:
        print(f"  ERROR: Server not reachable: {e}")
        return False

def main():
    print(f"\n{'#'*90}")
    print(f"  THE-BRIDGE — HTTP PERFORMANCE BENCHMARK SUITE")
    print(f"  Server: {API_BASE}")
    print(f"  Date: {time.strftime('%Y-%m-%d %H:%M:%S UTC', time.gmtime())}")
    print(f"{'#'*90}")

    if not check_server():
        sys.exit(1)

    benchmark_sequential()
    benchmark_concurrent()
    benchmark_multi_pair()
    benchmark_full_pipeline()
    benchmark_concurrent_clients()

    print(f"\n{'='*90}")
    print(f"  HTTP BENCHMARK SUITE COMPLETE")
    print(f"{'='*90}")
    print()

if __name__ == "__main__":
    main()
