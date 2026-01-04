# Solana Cross-Margin Settlement

A Solana smart contract implementation for settling derivative positions using cross-margin accounting with unreliable oracle conditions.

## Overview

This project implements a critical on-chain settlement function for a Solana-based derivatives DEX. The system handles:

- **Cross-margin collateral management** - Shared collateral pool across positions
- **Unrealized PnL settlement** - Mark-to-market position valuation
- **Funding payments** - Periodic funding rate application
- **Oracle unreliability** - Safe handling of stale/delayed price feeds

## Features

- ✅ **Mathematically correct** PnL and funding calculations
- ✅ **Overflow-safe** arithmetic using checked operations and i128
- ✅ **Double-counting prevention** for both PnL and funding
- ✅ **Idempotent settlements** - safe to call multiple times
- ✅ **Cross-margin support** - negative collateral allowed
- ✅ **Comprehensive tests** - 10+ test cases covering edge cases
- ✅ **Event emission** - Full settlement details logged

## Project Structure

```
solana-cross-margin-settlement/
├── programs/
│   └── solana-cross-margin-settlement/
│       ├── src/
│       │   └── lib.rs              # Main program implementation
│       └── Cargo.toml              # Program dependencies
├── tests/
│   └── settlement_tests.rs         # Comprehensive test suite
├── Anchor.toml                     # Anchor configuration
├── Cargo.toml                      # Workspace configuration
├── DESIGN.md                       # Design document with Q&A
└── README.md                       # This file
```

## Installation

### Prerequisites

- Rust 1.70+
- Solana CLI 1.16+
- Anchor 0.29.0

### Setup

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Solana CLI
sh -c "$(curl -sSfL https://release.solana.com/stable/install)"

# Install Anchor
cargo install --git https://github.com/coral-xyz/anchor avm --locked --force
avm install 0.29.0
avm use 0.29.0

# Build the project
anchor build
```

## Usage

### Settlement Function

```rust
pub fn settle_cross_margin(
    ctx: Context<SettleCrossMargin>,
    oracle_price: i64,    // Current oracle price (may be stale)
    funding_rate: i64,    // Current funding rate (signed)
) -> Result<()>
```

### Account Context

```rust
pub struct SettleCrossMargin<'info> {
    #[account(mut)]
    pub position: Account<'info, Position>,    // Position to settle

    #[account(mut)]
    pub balance: Account<'info, UserBalance>,  // Cross-margin balance

    pub authority: Signer<'info>,              // Settlement authority
}
```

### Example Settlement

```rust
// Long position at entry price 1000
Position {
    size: 100,              // 100 units long
    entry_price: 1000,      // Entered at 1000
    last_funding_rate: 10,  // Last funding rate
}

// Current state
oracle_price = 1100;        // Price increased to 1100
funding_rate = 15;          // Funding rate increased to 15

// Settlement calculation:
// PnL = (1100 - 1000) × 100 = 10,000
// Funding = (15 - 10) × 100 = 500
// Net = 10,000 - 500 = 9,500
// New collateral = old_collateral + 9,500

// After settlement:
Position {
    size: 100,
    entry_price: 1100,      // Updated to current price
    last_funding_rate: 15,  // Updated to current rate
}
```

## Testing

Run the test suite:

```bash
# Run unit tests
cargo test

# Run with verbose output
cargo test -- --nocapture
```

### Test Coverage

- ✅ Long position profit/loss
- ✅ Short position profit/loss
- ✅ Funding payment (positive/negative)
- ✅ Combined PnL and funding
- ✅ Double settlement prevention
- ✅ Zero position handling
- ✅ Negative collateral scenarios
- ✅ Overflow protection
- ✅ Large position safety

## Key Design Decisions

### 1. PnL Calculation (Mark-to-Market)
```
unrealized_pnl = (oracle_price - entry_price) × position_size
```
- Updates `entry_price` after settlement to prevent double-counting
- Long positions profit from price increases
- Short positions profit from price decreases

### 2. Funding Payment (Incremental)
```
funding_payment = (current_rate - last_rate) × position_size
```
- Only charges funding accrued since last settlement
- Updates `last_funding_rate` to prevent double-charging
- Positive rate: longs pay shorts
- Negative rate: shorts pay longs

### 3. Safety Mechanisms

**Overflow Protection:**
- All arithmetic uses `checked_*` operations
- Intermediate calculations use `i128` (wider than `i64`)
- Explicit errors on overflow/underflow

**Validations:**
- Oracle price must be positive
- Entry price must be positive
- Funding rates must be within bounds
- Zero-size positions handled gracefully

**Idempotency:**
- Multiple settlements with same price = no PnL
- Safe to retry failed transactions
- State updates are atomic

## Limitations & Trade-offs

### Oracle Trust
- ❌ No staleness check (requires timestamp validation)
- ❌ No price reasonableness bounds
- ✅ Assumes oracle is eventually accurate

### Funding Rate
- ❌ Calculated off-chain (trusted input)
- ✅ Bounded to prevent overflow
- ✅ Incrementally applied (no double-counting)

### Liquidation
- ❌ No maintenance margin enforcement
- ❌ No automatic liquidation trigger
- ✅ Allows negative collateral (cross-margin)

See [DESIGN.md](DESIGN.md) for detailed analysis of attacks, failure modes, and trustless limitations.

## Production Checklist

Before deploying to production:

- [ ] Add oracle staleness check with timestamp
- [ ] Implement price deviation limits
- [ ] Add maintenance margin enforcement
- [ ] Integrate liquidation mechanism
- [ ] Add emergency pause functionality
- [ ] Use TWAP oracle instead of spot price
- [ ] Implement keeper incentive mechanism
- [ ] Add insurance fund integration
- [ ] Complete security audit
- [ ] Formal verification of arithmetic

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Off-Chain System                     │
│  ┌─────────────┐  ┌──────────────┐  ┌───────────────┐  │
│  │   Oracle    │  │ Funding Rate │  │    Keeper     │  │
│  │  Provider   │  │  Calculator  │  │   Network     │  │
│  └──────┬──────┘  └──────┬───────┘  └───────┬───────┘  │
└─────────┼─────────────────┼──────────────────┼──────────┘
          │                 │                  │
          │ oracle_price    │ funding_rate     │ trigger
          │                 │                  │
          ▼                 ▼                  ▼
┌─────────────────────────────────────────────────────────┐
│              Solana On-Chain Program                    │
│                                                          │
│  ┌────────────────────────────────────────────────┐    │
│  │         settle_cross_margin()                  │    │
│  │                                                 │    │
│  │  1. Validate inputs                            │    │
│  │  2. Calculate PnL = (price - entry) × size     │    │
│  │  3. Calculate funding = (rate - last) × size   │    │
│  │  4. Update collateral += PnL - funding         │    │
│  │  5. Update position state (mark-to-market)     │    │
│  │  6. Emit settlement event                      │    │
│  └────────────────────────────────────────────────┘    │
│                                                          │
│  ┌──────────────┐  ┌─────────────────────────────┐     │
│  │   Position   │  │      UserBalance            │     │
│  │  - size      │  │  - collateral (cross-margin)│     │
│  │  - entry_price  │  └─────────────────────────────┘     │
│  │  - last_funding │                                   │
│  └──────────────┘                                      │
└─────────────────────────────────────────────────────────┘
```

## License

This is a test implementation for educational purposes.

## Resources

- [Anchor Framework Documentation](https://www.anchor-lang.com/)
- [Solana Developer Docs](https://docs.solana.com/)
- [DESIGN.md](DESIGN.md) - Detailed design analysis
- [Solana Program Library](https://spl.solana.com/)

## Contact

For questions about this implementation, please review the [DESIGN.md](DESIGN.md) document which contains detailed answers to common questions about the design, security, and trade-offs.