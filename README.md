# Index Wallets Backend

A Rust-based backend service for the Index Wallets platform, providing payment processing, token management, and cause donation functionality using the Delta blockchain.

## Quick Start

### 1. Setup Environment

```bash
# Copy example environment file
cp .env.example .env

# Edit .env with your configuration
```

### 2. Configure Private Keys

**For Local Development:**
```bash
# Generate keypair files
cargo run --bin generate_keys
```

**For Production:**
```bash
# Set environment variables
export CENTRAL_VAULT_PRIVATE_KEY="your_64_char_hex_key"
export NETWORK_GOODS_VAULT_PRIVATE_KEY="your_64_char_hex_key"
```

See [README_CONFIG.md](README_CONFIG.md) for detailed key management documentation.

### 3. Run the Server

```bash
# Development
cargo run

# Production
cargo run --release
```

## Features

- **Payment Processing**: QR code-based payments with bonding curve token calculations
- **Cause Management**: Create and manage fundraising causes with Stripe integration
- **Deposit Tracking**: Track USD and token deposits with complete transaction history
- **Token Management**: Multi-token support with vendor valuations and discounts
- **Webhook Integration**: Stripe webhook handling for payments and account updates

## API Endpoints

- `GET /api/users/{address}/transactions` - Get unified activity timeline
- `POST /api/payments` - Create payment requests
- `POST /api/payments/{id}/supplement` - Calculate payment bundles
- `GET /api/causes` - List available causes
- `POST /webhook/stripe` - Stripe webhook handler

## Configuration

The service supports flexible configuration via:
- Environment variables (production)
- JSON files (local development)
- Automatic fallback system

Key environment variables:
- `MONGODB_URI` - Database connection
- `STRIPE_SECRET_TEST` - Stripe API key
- `CENTRAL_VAULT_PRIVATE_KEY` - Main vault private key
- `NETWORK_GOODS_VAULT_PRIVATE_KEY` - Platform fee vault key

## Development

```bash
# Check code
cargo check

# Run tests
cargo test

# Format code
cargo fmt

# Generate new keypairs
cargo run --bin generate_keys
```

## Architecture

- **Models**: Data structures and API types
- **Services**: Business logic (MongoDB, Token, Webhook, etc.)
- **Handlers**: HTTP request handlers
- **Routes**: API route configuration
- **Utils**: Helper functions (bonding curves, payment calculations)

## Security

- Private keys loaded from environment variables in production
- JSON files for local development only
- No private keys committed to version control
- Stripe webhook signature verification