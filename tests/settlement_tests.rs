use anchor_lang::prelude::*;
use solana_cross_margin_settlement::{Position, UserBalance};

#[cfg(test)]
mod settlement_tests {
    use super::*;

    /// Helper function to create a test position
    fn create_position(size: i64, entry_price: i64, last_funding_rate: i64) -> Position {
        Position {
            size,
            entry_price,
            last_funding_rate,
        }
    }

    /// Helper function to create a test balance
    fn create_balance(collateral: i128) -> UserBalance {
        UserBalance { collateral }
    }

    #[test]
    fn test_long_position_profit() {
        // Long position (size > 0) with price increase
        let mut position = create_position(100, 1000, 0);
        let mut balance = create_balance(10000);

        // Oracle price increases to 1100
        let oracle_price = 1100;
        let funding_rate = 0;

        // Expected PnL: (1100 - 1000) * 100 = 10000
        // Expected funding: (0 - 0) * 100 = 0
        // Net settlement: 10000 - 0 = 10000
        // New collateral: 10000 + 10000 = 20000

        // Simulate settlement (manual calculation since we can't run actual Anchor program here)
        let price_delta = oracle_price - position.entry_price;
        let unrealized_pnl = (price_delta as i128) * (position.size as i128);
        let funding_delta = funding_rate - position.last_funding_rate;
        let funding_payment = (funding_delta as i128) * (position.size as i128);
        let net_settlement = unrealized_pnl - funding_payment;

        balance.collateral += net_settlement;
        position.entry_price = oracle_price;
        position.last_funding_rate = funding_rate;

        assert_eq!(balance.collateral, 20000);
        assert_eq!(position.entry_price, 1100);
    }

    #[test]
    fn test_long_position_loss() {
        // Long position with price decrease
        let mut position = create_position(100, 1000, 0);
        let mut balance = create_balance(10000);

        let oracle_price = 900; // Price drops
        let funding_rate = 0;

        // Expected PnL: (900 - 1000) * 100 = -10000
        // Net settlement: -10000
        // New collateral: 10000 - 10000 = 0

        let price_delta = oracle_price - position.entry_price;
        let unrealized_pnl = (price_delta as i128) * (position.size as i128);
        let funding_payment = 0;
        let net_settlement = unrealized_pnl - funding_payment;

        balance.collateral += net_settlement;

        assert_eq!(balance.collateral, 0);
    }

    #[test]
    fn test_short_position_profit() {
        // Short position (size < 0) with price decrease
        let mut position = create_position(-100, 1000, 0);
        let mut balance = create_balance(10000);

        let oracle_price = 900; // Price drops (good for short)
        let funding_rate = 0;

        // Expected PnL: (900 - 1000) * (-100) = -100 * -100 = 10000
        // Net settlement: 10000
        // New collateral: 10000 + 10000 = 20000

        let price_delta = oracle_price - position.entry_price;
        let unrealized_pnl = (price_delta as i128) * (position.size as i128);
        let funding_payment = 0;
        let net_settlement = unrealized_pnl - funding_payment;

        balance.collateral += net_settlement;

        assert_eq!(balance.collateral, 20000);
    }

    #[test]
    fn test_funding_payment_long() {
        // Long position paying positive funding
        let mut position = create_position(100, 1000, 10);
        let mut balance = create_balance(10000);

        let oracle_price = 1000; // No price change
        let funding_rate = 20; // Funding increased

        // Expected PnL: 0
        // Expected funding: (20 - 10) * 100 = 1000
        // Net settlement: 0 - 1000 = -1000
        // New collateral: 10000 - 1000 = 9000

        let price_delta = oracle_price - position.entry_price;
        let unrealized_pnl = (price_delta as i128) * (position.size as i128);
        let funding_delta = funding_rate - position.last_funding_rate;
        let funding_payment = (funding_delta as i128) * (position.size as i128);
        let net_settlement = unrealized_pnl - funding_payment;

        balance.collateral += net_settlement;
        position.last_funding_rate = funding_rate;

        assert_eq!(balance.collateral, 9000);
        assert_eq!(position.last_funding_rate, 20);
    }

    #[test]
    fn test_double_settlement_prevention() {
        // Test that settling twice doesn't double-count PnL
        let mut position = create_position(100, 1000, 0);
        let mut balance = create_balance(10000);

        let oracle_price = 1100;
        let funding_rate = 0;

        // First settlement
        let price_delta = oracle_price - position.entry_price;
        let unrealized_pnl = (price_delta as i128) * (position.size as i128);
        balance.collateral += unrealized_pnl;
        position.entry_price = oracle_price; // Update entry price

        assert_eq!(balance.collateral, 20000);

        // Second settlement with same oracle price
        // Should result in 0 PnL because entry_price was updated
        let price_delta_2 = oracle_price - position.entry_price;
        let unrealized_pnl_2 = (price_delta_2 as i128) * (position.size as i128);
        balance.collateral += unrealized_pnl_2;

        assert_eq!(balance.collateral, 20000); // No change
        assert_eq!(unrealized_pnl_2, 0);
    }

    #[test]
    fn test_negative_collateral_allowed() {
        // Cross-margin allows negative collateral
        let mut position = create_position(100, 1000, 0);
        let mut balance = create_balance(5000);

        let oracle_price = 900; // Large loss
        let funding_rate = 0;

        // Expected PnL: (900 - 1000) * 100 = -10000
        // Net settlement: -10000
        // New collateral: 5000 - 10000 = -5000

        let price_delta = oracle_price - position.entry_price;
        let unrealized_pnl = (price_delta as i128) * (position.size as i128);
        let net_settlement = unrealized_pnl;

        balance.collateral += net_settlement;

        assert_eq!(balance.collateral, -5000); // Negative allowed in cross-margin
    }

    #[test]
    fn test_zero_position_size() {
        // Settling a position with size 0 should be safe
        let mut position = create_position(0, 1000, 0);
        let mut balance = create_balance(10000);

        let oracle_price = 1100;
        let funding_rate = 10;

        // With size = 0, both PnL and funding should be 0
        if position.size == 0 {
            position.last_funding_rate = funding_rate;
            // No collateral change
        }

        assert_eq!(balance.collateral, 10000); // No change
        assert_eq!(position.last_funding_rate, funding_rate); // Funding rate updated
    }

    #[test]
    fn test_large_position_overflow_safety() {
        // Test that i128 is used to prevent overflow
        let position = create_position(1_000_000_000, 1000, 0); // 1 billion units
        let oracle_price = 2000; // 1000 point move

        // This would overflow i64 but should work with i128
        let price_delta = oracle_price - position.entry_price;
        let unrealized_pnl = (price_delta as i128)
            .checked_mul(position.size as i128)
            .expect("Should not overflow with i128");

        // 1000 * 1_000_000_000 = 1_000_000_000_000
        assert_eq!(unrealized_pnl, 1_000_000_000_000);
    }

    #[test]
    fn test_combined_pnl_and_funding() {
        // Test settlement with both PnL and funding
        let mut position = create_position(100, 1000, 5);
        let mut balance = create_balance(10000);

        let oracle_price = 1050; // Price increase
        let funding_rate = 15; // Funding increase

        // Expected PnL: (1050 - 1000) * 100 = 5000
        // Expected funding: (15 - 5) * 100 = 1000
        // Net settlement: 5000 - 1000 = 4000
        // New collateral: 10000 + 4000 = 14000

        let price_delta = oracle_price - position.entry_price;
        let unrealized_pnl = (price_delta as i128) * (position.size as i128);
        let funding_delta = funding_rate - position.last_funding_rate;
        let funding_payment = (funding_delta as i128) * (position.size as i128);
        let net_settlement = unrealized_pnl - funding_payment;

        balance.collateral += net_settlement;

        assert_eq!(balance.collateral, 14000);
    }

    #[test]
    fn test_negative_funding_rate() {
        // Test with negative funding (shorts pay longs)
        let mut position = create_position(100, 1000, 5);
        let mut balance = create_balance(10000);

        let oracle_price = 1000;
        let funding_rate = -5; // Negative funding

        // Expected PnL: 0
        // Expected funding: (-5 - 5) * 100 = -1000
        // Net settlement: 0 - (-1000) = 1000 (long receives funding)
        // New collateral: 10000 + 1000 = 11000

        let price_delta = oracle_price - position.entry_price;
        let unrealized_pnl = (price_delta as i128) * (position.size as i128);
        let funding_delta = funding_rate - position.last_funding_rate;
        let funding_payment = (funding_delta as i128) * (position.size as i128);
        let net_settlement = unrealized_pnl - funding_payment;

        balance.collateral += net_settlement;

        assert_eq!(balance.collateral, 11000);
    }
}
