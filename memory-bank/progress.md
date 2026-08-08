# Progress

## Phase 1.5: Arbitrage Engine Activation (2026-07-29)
- Cross-Venue Arbitrage wired into API server (`/api/v1/arb/cross-venue/*`)
- Super-Arb Engine wired into API server (`/api/v1/arb/super/*`)
- Both engines auto-start on server boot with 5s and 8s delays
- CI/CD pipeline with check/fmt/test/build/docker/deploy stages
- Dockerfile for production container builds
- Criterion benchmark suite for order placement, matching latency, concurrent writes
