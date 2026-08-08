# Configuration Management System

## Overview

This is the **Config Manager Dashboard** for THE-BRIDGE matching engine. It provides a centralized interface for managing:
- Network configurations (Ethereum, Arbitrum, BSC, Polygon, etc.)
- Provider configurations (Alchemy, Flashbots, Binance, Coinbase, etc.)
- Engine configurations (Flash Loan Arbitrage, MEV Extraction, etc.)
- Secret management and rotation
- Backup and recovery
- System monitoring and diagnostics

## Key Features

### 🔐 Secure Configuration Management
- **Encryption**: All secrets are AES-256-GCM encrypted at rest
- **Centralized Storage**: Single source of truth for all configurations
- **Role-based Access Control**: Different permission levels for different users
- **Audit Logging**: Full audit trail of all configuration changes

### 🌐 Multi-Network Support
- **EVM Networks**: Ethereum Mainnet, Arbitrum, BSC, Polygon, Optimism, Base, etc.
- **Network Metadata**: RPC URLs, chain IDs, gas sponsorship, native currency
- **Network Priority**: Configurable network priority for failover scenarios

### 🔌 Provider Management
- **API Provider Integration**: Alchemy, Flashbots, Binance, Coinbase, QuickNode, Ankr
- **Custom Providers**: Support for custom RPC endpoints and services
- **Rate Limiting**: Configurable rate limits per provider
- **Health Monitoring**: Real-time provider health checks

### ⚙️ Engine Configuration
- **Trading Engines**: Flash Loan Arbitrage, MEV Extraction, Cross-Venue Arbitrage, Super Arb
- **Revenue Engines**: Revenue distribution, FX trading, liquidity provision, risk management
- **Advanced Modules**: WASM Policy, Dark Pool, Compliance, Onboarding
- **Performance**: NUMA-aware thread pools, HugePages, WASM hooks

### 📊 Monitoring and Diagnostics
- **Real-time Metrics**: Engine performance, configuration status, system resources
- **Health Checks**: Comprehensive health monitoring of all components
- **Audit Trail**: Complete logging of all configuration changes
- **Alerting**: Configurable alerts for configuration issues

## Usage

### Quick Start

1. **Start the Config Manager Dashboard**
   ```bash
   cd local-additions/config-manager
   cargo run --bin config-manager
   ```

2. **Access the Dashboard**
   - Web Interface: `http://localhost:8080`
   - API Documentation: `http://localhost:8080/api/docs`

3. **Initial Setup**
   - Create your first network configuration
   - Add your API provider credentials (encrypted)
   - Configure your trading engines

### API Endpoints

#### Configuration Management
- `GET /api/v1/config` - Get current configuration
- `POST /api/v1/config` - Save new configuration
- `PUT /api/v1/config` - Update existing configuration
- `DELETE /api/v1/config` - Reset to defaults

#### Network Configuration
- `GET /api/v1/config/networks` - List all networks
- `POST /api/v1/config/networks` - Add new network
- `GET /api/v1/config/networks/{network_id}` - Get network details
- `PUT /api/v1/config/networks/{network_id}` - Update network
- `DELETE /api/v1/config/networks/{network_id}` - Remove network

#### Provider Configuration
- `GET /api/v1/config/providers` - List all providers
- `POST /api/v1/config/providers` - Add new provider
- `GET /api/v1/config/providers/{provider_name}` - Get provider details
- `PUT /api/v1/config/providers/{provider_name}` - Update provider
- `DELETE /api/v1/config/providers/{provider_name}` - Remove provider

#### Engine Configuration
- `GET /api/v1/config/engines` - List all engines
- `POST /api/v1/config/engines` - Add new engine
- `GET /api/v1/config/engines/{engine_name}` - Get engine details
- `PUT /api/v1/config/engines/{engine_name}` - Update engine
- `DELETE /api/v1/config/engines/{engine_name}` - Remove engine

#### Secret Management
- `GET /api/v1/config/secrets/{secret_type}` - List secrets
- `POST /api/v1/config/secrets/{secret_type}/rotate` - Rotate secret
- `POST /api/v1/config/secrets/{secret_type}/test` - Test secret connectivity

#### System Operations
- `GET /health` - Health check
- `GET /status` - Detailed system status
- `POST /api/v1/config/backup` - Create backup
- `POST /api/v1/config/validate` - Validate configuration

## Configuration Examples

### Network Configuration
```json
{
  "name": "Arbitrum Mainnet",
  "id": "arbitrum-mainnet",
  "chain_id": 42161,
  "rpc_url": "https://arb-mainnet.g.alchemy.com/v2/YOUR_API_KEY",
  "ws_url": "wss://arb-mainnet.g.alchemy.com/v2/YOUR_API_KEY",
  "explorer_url": "https://arbiscan.io",
  "native_currency": {
    "name": "Ether",
    "symbol": "ETH",
    "decimals": 18
  },
  "gas_sponsorship_id": "93a99dac-c52c-4261-95ba-7817749a3e08",
  "enabled": true,
  "priority": 1
}
```

### Provider Configuration
```json
{
  "name": "Alchemy",
  "provider_type": "Alchemy",
  "api_key": {
    "value": "***",
    "encrypted": true
  },
  "api_secret": null,
  "api_url": "https://eth-mainnet.g.alchemy.com/v2",
  "supported_networks": ["ethereum", "arbitrum", "polygon"],
  "rate_limit": {
    "requests_per_minute": 100,
    "requests_per_hour": 10000,
    "burst_limit": 20
  },
  "enabled": true,
  "priority": 1
}
```

### Engine Configuration
```json
{
  "name": "MEV Extraction",
  "engine_type": "MevExtraction",
  "enabled": true,
  "provider": "Flashbots",
  "network": "ethereum",
  "parameters": {
    "min_profit_usd": 100,
    "max_gas_price_gwei": 200,
    "scan_interval_ms": 1000,
    "max_concurrent_bundles": 5
  },
  "monitoring": {
    "metrics_enabled": true,
    "logs_enabled": true,
    "alerts_enabled": true,
    "dashboard_url": "http://localhost:3001/api/v1/mev/status",
    "webhook_url": "http://your-server.com/webhooks/mev"
  }
}
```

## Security Features

### Encryption
- **Algorithm**: AES-256-GCM for all secret storage
- **Key Management**: Hierarchical key management with rotation support
- **IV Generation**: Cryptographically secure IVs for each encryption operation
- **Key Derivation**: PBKDF2 for master password to encryption key derivation

### Access Control
- **Authentication**: JWT-based authentication for API endpoints
- **Authorization**: Role-based access control (Admin, Operator, Viewer)
- **IP Restrictions**: Configurable IP whitelisting
- **Audit Logging**: Complete audit trail of all access and configuration changes

### Backup and Recovery
- **Automated Backups**: Scheduled backups with configurable retention
- **Point-in-Time Recovery**: Ability to restore previous configuration states
- **Encrypted Backups**: All backups are encrypted at rest
- **Multi-location Storage**: Local storage and optional S3 integration

## Integration with Matching Engine

The config manager integrates seamlessly with the THE-BRIDGE matching engine:

### Configuration Loading
- **Dynamic Loading**: Engine loads configuration on startup and periodically
- **Validation**: Validates all configuration before applying
- **Fallbacks**: Multiple fallback mechanisms for configuration failure scenarios

### Runtime Updates
- **Live Updates**: Support for runtime configuration updates
- **Graceful Degradation**: Fallback to safe defaults on configuration errors
- **Hot Reload**: Support for hot reloading of configuration without restart

### Monitoring Integration
- **Metrics Collection**: Integration with engine's monitoring system
- **Health Checks**: Continuous monitoring of configuration validity
- **Alerting**: Configurable alerts for configuration issues

## Development and Deployment

### Development Setup
```bash
# Clone the project
cd the-bridge

# Navigate to config-manager
cd local-additions/config-manager

# Run tests
cargo test

# Run in development mode
cargo run
```

### Production Deployment
```bash
# Build optimized release
cargo build --release

# Run with optimized settings
cargo run --release

# Deploy as Docker container
# See docker/ directory for example Dockerfile
```

### Configuration Templates

#### Example config.toml
```toml
[networks.ethereum]
rpc_url = "https://eth-mainnet.g.alchemy.com/v2/***"
chain_id = 1
gas_sponsorship_id = "***"

[providers.alchemy]
api_key = "***"
supported_networks = ["ethereum", "arbitrum"]
rate_limit.requests_per_minute = 100

[engines.mev_extraction]
enabled = true
provider = "Flashbots"
network = "ethereum"
parameters.min_profit_usd = 100
```

## Monitoring and Observability

### Metrics
- **Configuration Load Time**: Time taken to load configuration
- **Secret Encryption/Decryption**: Performance of encryption operations
- **API Response Times**: Response times for all configuration endpoints
- **Error Rates**: Error rates for configuration operations

### Alerts
- **Configuration Changes**: Alert on critical configuration changes
- **Secret Rotation**: Alert when secrets are rotated
- **Backup Failures**: Alert on backup failures
- **Validation Errors**: Alert on configuration validation failures

## Troubleshooting

### Common Issues

#### Configuration Not Loading
```bash
# Check if config file exists
ls -la /app/config/config.toml

# Check for encryption errors
tail -f /var/log/config-manager.log

# Verify storage directory
ls -la /app/config/
```

#### API Endpoints Not Working
```bash
# Check if server is running
curl http://localhost:8080/health

# Check logs
tail -f /var/log/axum.log

# Verify network connectivity
curl http://localhost:8080/api/v1/config
```

#### Secret Encryption Issues
```bash
# Verify master password is correct
# Check for backup files
cp /app/config/config.toml.backup /tmp/config.toml
```

## Support

For issues and support, please:
1. Check the logs at `/var/log/config-manager.log`
2. Visit the GitHub repository issues
3. Check the documentation at `docs/`
4. Contact support@the-bridge.io

## License

This configuration management system is part of THE-BRIDGE matching engine, licensed under the AGPL v3 license.

---

*This documentation is generated from the source code and may be updated automatically.*
