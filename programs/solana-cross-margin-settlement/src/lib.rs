use anchor_lang::prelude::*;

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[program]
pub mod solana_cross_margin_settlement {
    use super::*;

    /// Settles cross-margin positions by applying unrealized PnL and funding payments
    ///
    /// # Arguments
    /// * `oracle_price` - Current oracle price (may be stale/unreliable)
    /// * `funding_rate` - Current funding rate (signed, per position unit)
    ///
    /// # Safety Considerations
    /// - Oracle price may be stale, delayed, or incorrect
    /// - Funding rate may lag price updates
    /// - Settlement can be called multiple times
    /// - Must prevent integer overflow/underflow
    /// - Must prevent double-counting of PnL or funding
    pub fn settle_cross_margin(
        ctx: Context<SettleCrossMargin>,
        oracle_price: i64,
        funding_rate: i64,
    ) -> Result<()> {
        let position = &mut ctx.accounts.position;
        let balance = &mut ctx.accounts.balance;

        // ============================================================================
        // VALIDATIONS
        // ============================================================================

        // 1. Validate oracle price is positive (can't have negative prices)
        require!(oracle_price > 0, SettlementError::InvalidOraclePrice);

        // 2. Validate entry price is positive (sanity check on position state)
        require!(position.entry_price > 0, SettlementError::InvalidEntryPrice);

        // 3. Check if position has size (no point settling empty position)
        if position.size == 0 {
            // No position to settle, but update funding rate to prevent future issues
            position.last_funding_rate = funding_rate;
            return Ok(());
        }

        // 4. Validate funding rate is within reasonable bounds to prevent overflow
        // This is a safety check - in production, you'd have protocol-specific bounds
        const MAX_FUNDING_RATE: i64 = i64::MAX / 1_000_000; // Arbitrary reasonable bound
        require!(
            funding_rate.abs() <= MAX_FUNDING_RATE,
            SettlementError::FundingRateOutOfBounds
        );
        require!(
            position.last_funding_rate.abs() <= MAX_FUNDING_RATE,
            SettlementError::FundingRateOutOfBounds
        );

        // ============================================================================
        // UNREALIZED PnL CALCULATION
        // ============================================================================

        // Calculate price delta with overflow protection
        let price_delta = oracle_price
            .checked_sub(position.entry_price)
            .ok_or(SettlementError::CalculationOverflow)?;

        // Calculate unrealized PnL: (oracle_price - entry_price) * size
        // Use i128 to prevent overflow during multiplication
        let unrealized_pnl = (price_delta as i128)
            .checked_mul(position.size as i128)
            .ok_or(SettlementError::CalculationOverflow)?;

        // ============================================================================
        // FUNDING PAYMENT CALCULATION
        // ============================================================================

        // Calculate funding delta (only pay funding accrued since last settlement)
        // This prevents double-counting of funding
        let funding_delta = funding_rate
            .checked_sub(position.last_funding_rate)
            .ok_or(SettlementError::CalculationOverflow)?;

        // Calculate funding payment: (funding_rate - last_funding_rate) * size
        // Positive funding_rate means longs pay shorts (reduces long collateral)
        // Use i128 to prevent overflow
        let funding_payment = (funding_delta as i128)
            .checked_mul(position.size as i128)
            .ok_or(SettlementError::CalculationOverflow)?;

        // ============================================================================
        // COLLATERAL UPDATE (CROSS-MARGIN)
        // ============================================================================

        // Net settlement = PnL - funding_payment
        // If size > 0 (long): positive PnL increases collateral, positive funding decreases it
        // If size < 0 (short): negative PnL increases collateral, negative funding decreases it
        let net_settlement = unrealized_pnl
            .checked_sub(funding_payment)
            .ok_or(SettlementError::CalculationOverflow)?;

        // Apply to cross-margin collateral with overflow protection
        let new_collateral = balance.collateral
            .checked_add(net_settlement)
            .ok_or(SettlementError::CalculationOverflow)?;

        // Update collateral (can be negative in cross-margin)
        // Note: Allowing negative balance for cross-margin
        // In production, you'd check against maintenance margin requirements
        balance.collateral = new_collateral;

        // ============================================================================
        // STATE UPDATES (PREVENT DOUBLE-COUNTING)
        // ============================================================================

        // Update position state to reflect settlement
        // This prevents double-counting on subsequent settlements
        position.entry_price = oracle_price; // Mark-to-market
        position.last_funding_rate = funding_rate; // Update funding checkpoint

        // ============================================================================
        // EMIT EVENT FOR MONITORING
        // ============================================================================

        emit!(SettlementEvent {
            position_key: position.key(),
            oracle_price,
            funding_rate,
            unrealized_pnl,
            funding_payment,
            net_settlement,
            new_collateral,
        });

        Ok(())
    }
}

// ============================================================================
// ACCOUNT STRUCTURES
// ============================================================================

#[account]
pub struct Position {
    /// Signed position size (positive = long, negative = short)
    pub size: i64,
    /// Entry price (used as reference for PnL calculation)
    pub entry_price: i64,
    /// Last settled funding rate (prevents double-counting)
    pub last_funding_rate: i64,
}

#[account]
pub struct UserBalance {
    /// Shared cross-margin collateral (can be negative)
    pub collateral: i128,
}

// ============================================================================
// CONTEXT
// ============================================================================

#[derive(Accounts)]
pub struct SettleCrossMargin<'info> {
    #[account(mut)]
    pub position: Account<'info, Position>,

    #[account(mut)]
    pub balance: Account<'info, UserBalance>,

    /// Authority that can trigger settlement (e.g., user or keeper)
    pub authority: Signer<'info>,
}

// ============================================================================
// ERRORS
// ============================================================================

#[error_code]
pub enum SettlementError {
    #[msg("Oracle price must be positive")]
    InvalidOraclePrice,

    #[msg("Entry price must be positive")]
    InvalidEntryPrice,

    #[msg("Calculation resulted in overflow or underflow")]
    CalculationOverflow,

    #[msg("Funding rate is outside acceptable bounds")]
    FundingRateOutOfBounds,
}

// ============================================================================
// EVENTS
// ============================================================================

#[event]
pub struct SettlementEvent {
    pub position_key: Pubkey,
    pub oracle_price: i64,
    pub funding_rate: i64,
    pub unrealized_pnl: i128,
    pub funding_payment: i128,
    pub net_settlement: i128,
    pub new_collateral: i128,
}
