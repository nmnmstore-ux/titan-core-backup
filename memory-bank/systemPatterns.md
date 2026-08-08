# System Patterns

## API Patterns
- **Arbitrage routes** follow a read-only GET pattern for stats/pnl/trades/prices:
  - `/api/v1/arb/cross-venue/{stats,pnl,trades,prices}`
  - `/api/v1/arb/super/{stats,pnl,trades,prices}`
- Pattern: GET routes for read-only data, spawned as background tokio tasks
