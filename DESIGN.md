# Cross-Margin Settlement Design Document

## Overview

This document outlines the design decisions, safety mechanisms, and trade-offs for the `settle_cross_margin` function implemented for a Solana-based derivatives DEX with cross-margining support.

## Core Settlement Logic

### Unrealized PnL Calculation
```
unrealized_pnl = (oracle_price - entry_price) × position_size
```

- **Long positions** (size > 0): Profit when price increases
- **Short positions** (size < 0): Profit when price decreases
- Uses `i128` arithmetic to prevent overflow on large positions

### Funding Payment Calculation
```
funding_payment = (current_funding_rate - last_funding_rate) × position_size
```

- Only charges incremental funding since last settlement
- Positive funding rate: longs pay shorts (reduces long collateral)
- Negative funding rate: shorts pay longs (increases long collateral)
- Prevents double-counting by tracking `last_funding_rate`

### Net Settlement
```
net_settlement = unrealized_pnl - funding_payment
collateral_new = collateral_old + net_settlement
```

## Design Questions & Answers

### 1. What validations would you perform before applying PnL and funding?

**Implemented Validations:**

1. **Oracle Price Validation**
   - Must be positive (> 0)
   - Prevents undefined behavior from negative prices
   - Does NOT validate freshness (discussed below in limitations)

2. **Entry Price Validation**
   - Must be positive (> 0)
   - Sanity check on position state integrity
   - Detects corrupted position data

3. **Funding Rate Bounds Check**
   - Both current and last funding rates must be within reasonable bounds
   - Prevents overflow during funding calculation
   - Bounds: `|funding_rate| ≤ i64::MAX / 1_000_000`

4. **Position Size Check**
   - Zero-size positions are handled gracefully (no-op except funding rate update)
   - Non-zero positions proceed with settlement

5. **Overflow Protection**
   - All arithmetic operations use `checked_*` methods
   - Intermediate calculations promoted to `i128`
   - Explicit error on overflow/underflow

**Validations NOT Performed (Trade-offs):**

- **Oracle staleness check**: Requires timestamp oracle or external validation
- **Price reasonableness check**: Would need historical price data
- **Maintenance margin check**: Requires protocol-level margin requirements
- **Authorization check**: Minimal in skeleton (only `authority: Signer`)

### 2. What assumptions are unavoidable in this design?

**Critical Assumptions:**

1. **Oracle Trust with Delays**
   - Assumption: Oracle price is eventually accurate, though may be delayed
   - Impact: Temporary mispricing is accepted; settlement can happen at stale prices
   - Mitigation: Off-chain monitoring, liquidation mechanisms, price deviation bounds

2. **Funding Rate Integrity**
   - Assumption: Funding rates are calculated correctly off-chain
   - Impact: Incorrect funding rates will be applied to positions
   - Mitigation: Off-chain validation, governance controls, rate bounds

3. **Settlement Timing**
   - Assumption: Settlement can be triggered at any time by any authorized party
   - Impact: Race conditions possible if multiple parties settle simultaneously
   - Mitigation: Idempotent design (repeated settlements with same price = no change)

4. **Collateral Sufficiency**
   - Assumption: Cross-margin model allows negative collateral temporarily
   - Impact: Positions can be underwater without immediate liquidation
   - Mitigation: Separate liquidation mechanism required (not in scope)

5. **Single Position Per Account**
   - Assumption: Each Position account represents one market position
   - Impact: Multi-market cross-margining requires separate aggregation
   - Mitigation: Portfolio-level margin calculation needed externally

6. **No Partial Settlement**
   - Assumption: Entire position is settled atomically
   - Impact: Cannot settle portion of position
   - Mitigation: Position splitting would require additional instructions

### 3. How would you prevent double-counting of funding or PnL?

**Double-Counting Prevention Mechanisms:**

1. **Funding Double-Count Prevention**
   ```rust
   funding_delta = current_funding_rate - last_funding_rate
   funding_payment = funding_delta × position_size
   position.last_funding_rate = current_funding_rate  // Update checkpoint
   ```

   - Tracks `last_funding_rate` per position
   - Only charges incremental funding since last settlement
   - Updates checkpoint after each settlement
   - **Test case**: `test_double_settlement_prevention()`

2. **PnL Double-Count Prevention**
   ```rust
   unrealized_pnl = (oracle_price - entry_price) × position_size
   position.entry_price = oracle_price  // Mark-to-market
   ```

   - Updates `entry_price` to current oracle price after settlement
   - Subsequent settlement with same price yields zero PnL
   - Effectively "realizes" the PnL by marking to market
   - **Test case**: `test_double_settlement_prevention()`

3. **Idempotent Settlement**
   - Calling settlement multiple times with same parameters is safe
   - No state change if price and funding haven't changed
   - Critical for handling retries and race conditions

**Edge Case - Multiple Settlements in Same Slot:**

If two settlement transactions execute in the same slot with different prices:
- First settlement: Applies PnL from entry_price to price_1
- Second settlement: Applies PnL from price_1 to price_2
- **Result**: Correctly reflects both price movements
- **No double-counting** because entry_price is updated after each settlement

### 4. What attacks or failure modes are still possible?

**Attack Vectors:**

1. **Oracle Manipulation**
   - **Attack**: Attacker manipulates oracle to report false price
   - **Impact**: Incorrect PnL settlement, theft of collateral
   - **Mitigation**:
     - Use time-weighted average prices (TWAP)
     - Multiple oracle sources with median
     - Price deviation circuit breakers
     - Delayed settlement to allow dispute period

2. **Funding Rate Manipulation**
   - **Attack**: Off-chain component provides manipulated funding rates
   - **Impact**: Unfair funding payments
   - **Mitigation**:
     - On-chain funding rate calculation (adds complexity)
     - Governance oversight of funding rate parameters
     - Rate change limits per time period

3. **Settlement Timing Attacks**
   - **Attack**: Repeatedly settle at favorable prices, skip unfavorable ones
   - **Impact**: Cherry-picking settlement times for profit
   - **Mitigation**:
     - Forced settlement mechanism (liquidation)
     - Minimum settlement frequency
     - Keeper incentives for regular settlement

4. **Flash Crash Exploitation**
   - **Attack**: Trigger settlement during momentary oracle price spike
   - **Impact**: Unfair liquidations or PnL realization
   - **Mitigation**:
     - TWAP instead of spot price
     - Settlement delay/cooldown
     - Maximum price movement per settlement

5. **Integer Overflow/Underflow**
   - **Attack**: Create extreme positions to cause overflow
   - **Impact**: Undefined behavior, potential collateral theft
   - **Mitigation**: ✅ **Fully mitigated** via checked arithmetic and i128 usage

**Failure Modes:**

1. **Stale Oracle Price**
   - **Scenario**: Oracle stops updating or lags significantly
   - **Impact**: Settlement at incorrect prices
   - **Detection**: Off-chain monitoring, timestamp checks
   - **Recovery**: Manual intervention, circuit breaker

2. **Negative Collateral Spiral**
   - **Scenario**: Losses exceed collateral, position cannot be liquidated
   - **Impact**: Bad debt accumulation
   - **Detection**: Margin monitoring
   - **Recovery**: Socialized loss mechanism, insurance fund

3. **Concurrent Settlement Race**
   - **Scenario**: Multiple parties try to settle same position simultaneously
   - **Impact**: Last writer wins, earlier settlements may be wasted
   - **Detection**: Transaction logs show conflicts
   - **Recovery**: Retry mechanism, nonce-based ordering

### 5. What parts of this logic cannot be made fully trustless on-chain?

**Inherently Trusted Components:**

1. **Oracle Price Feed**
   - **Why**: Real-world price data originates off-chain
   - **Trust**: Must trust oracle provider or aggregation mechanism
   - **Decentralization**: Can improve with multiple oracles, but never fully trustless
   - **Best Practice**: Pyth, Switchboard, Chainlink with multiple publishers

2. **Funding Rate Calculation**
   - **Why**: Requires tracking order book imbalance, open interest, time-weighted positions
   - **Trust**: Must trust off-chain calculation or accept on-chain computation cost
   - **Decentralization**: Could be computed on-chain but very expensive
   - **Best Practice**: Transparent formula, governance oversight, on-chain verification

3. **Settlement Trigger Timing**
   - **Why**: Someone must decide when to call settlement
   - **Trust**: Relies on external actors (users, keepers) to trigger
   - **Decentralization**: Permissionless settlement helps but timing still discretionary
   - **Best Practice**: Keeper network with incentives, forced settlement on liquidation

4. **Oracle Staleness/Validity**
   - **Why**: Cannot prove on-chain that price is recent and accurate
   - **Trust**: Must trust timestamp and price freshness
   - **Decentralization**: Multiple oracles reduce but don't eliminate trust
   - **Best Practice**: Confidence intervals, multiple sources, outlier rejection

5. **Maintenance Margin Parameters**
   - **Why**: Risk parameters are subjective and market-dependent
   - **Trust**: Governance must set appropriate thresholds
   - **Decentralization**: On-chain governance can vote, but still not "trustless"
   - **Best Practice**: DAO governance, gradual parameter changes

**Partially Trustless Components:**

1. **PnL Calculation** ✅
   - Given oracle price, calculation is deterministic and verifiable
   - Trust only extends to oracle input

2. **Collateral Accounting** ✅
   - State transitions are fully on-chain and auditable
   - No trust required for arithmetic correctness

3. **Double-Counting Prevention** ✅
   - Logic is transparent and verifiable on-chain
   - No off-chain dependencies

## Safety Mechanisms Summary

| Mechanism | Implementation | Trade-off |
|-----------|---------------|-----------|
| Overflow Protection | `checked_*` operations + i128 | Small gas cost increase |
| Oracle Validation | Positivity check only | No staleness detection |
| Funding Bounds | Max rate limits | Arbitrary bounds chosen |
| Mark-to-Market | Entry price update | Prevents PnL re-use |
| Funding Checkpoint | Last rate tracking | Prevents double-charge |
| Zero Position Handling | Early return | Gas optimization |
| Negative Collateral | Allowed in cross-margin | Requires external liquidation |
| Event Emission | Full settlement details | Monitoring & transparency |

## Production Readiness Checklist

To make this production-ready, add:

- [ ] Oracle staleness check (require timestamp within threshold)
- [ ] Price deviation limits (max % change per settlement)
- [ ] Maintenance margin enforcement
- [ ] Liquidation mechanism integration
- [ ] Multi-signature authority for admin functions
- [ ] Emergency circuit breaker (pause settlement)
- [ ] TWAP oracle integration
- [ ] Keeper incentive mechanism
- [ ] Insurance fund integration
- [ ] Socialized loss handling
- [ ] Position size limits (prevent overflow risk)
- [ ] Comprehensive integration tests
- [ ] Formal verification of arithmetic
- [ ] Security audit

## Conclusion

This implementation prioritizes **correctness** and **safety** over features:

✅ **Strengths:**
- Mathematically correct PnL and funding calculations
- Robust overflow protection
- Double-counting prevention
- Idempotent settlement
- Clear state transitions

⚠️ **Limitations:**
- Oracle trust assumptions
- No liquidation mechanism
- Minimal authorization checks
- No staleness validation
- Requires external monitoring

The design makes explicit trade-offs appropriate for a cross-margin settlement function in a derivatives protocol where some components (oracle, funding rate) necessarily remain off-chain or semi-trusted.
