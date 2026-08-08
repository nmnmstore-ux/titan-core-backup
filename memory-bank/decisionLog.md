# Decision Log

## Decision: Use cross-venue-arb and super-arb as first arbitrage integrations
- **Rationale**: Both crates are standalone (no internal deps), have tests, and compile independently
- **Trade-off**: `flash_loan_arb` and `mev_extraction_engine` depend on sibling crates (`core`, `flash-loan`) making them harder to wire; deferred to Phase 2
- **Impact**: 2 of 4 engines active immediately; 8 API routes added
