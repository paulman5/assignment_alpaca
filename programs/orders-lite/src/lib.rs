use anchor_lang::prelude::*;

declare_id!("DB2h5equ9Qp2qaaeKyL9sL6RD8LAhZzjiSUYgnaW3aeF");

// Stripped-down spoutOrders: the order lifecycle only. The production program adds
// KYC gating, oracle price validation, and USDC/token escrows around this same shape.
// A settled or refunded order's PDA is closed — retrying fulfill/refund on it fails
// with AccountNotInitialized, which is the on-chain "already_settled" signal.

#[program]
pub mod orders_lite {
    use super::*;

    pub fn place_buy_order(
        ctx: Context<PlaceBuyOrder>,
        order_id: u64,
        ticker: String,
        usdc_amount: u64,
    ) -> Result<()> {
        require!(ticker.len() <= MAX_TICKER_LEN, OrdersLiteError::TickerTooLong);
        require!(usdc_amount > 0, OrdersLiteError::ZeroAmount);

        let order = &mut ctx.accounts.pending_order;
        order.user = ctx.accounts.user.key();
        order.order_id = order_id;
        order.ticker = ticker.clone();
        order.usdc_amount = usdc_amount;
        order.created_at = Clock::get()?.unix_timestamp;
        order.bump = ctx.bumps.pending_order;

        emit!(BuyOrderCreated {
            user: order.user,
            order_id,
            ticker,
            usdc_amount,
        });
        Ok(())
    }

    pub fn fulfill_buy_order(
        ctx: Context<CloseOrder>,
        order_id: u64,
        actual_usdc: u64,
    ) -> Result<()> {
        emit!(BuyOrderFulfilled {
            user: ctx.accounts.user.key(),
            order_id,
            actual_usdc,
        });
        Ok(())
    }

    pub fn refund_buy_order(ctx: Context<CloseOrder>, order_id: u64) -> Result<()> {
        emit!(BuyOrderRefunded {
            user: ctx.accounts.user.key(),
            order_id,
        });
        Ok(())
    }
}

pub const MAX_TICKER_LEN: usize = 8;

#[derive(Accounts)]
#[instruction(order_id: u64)]
pub struct PlaceBuyOrder<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        init,
        payer = user,
        space = PendingOrder::SPACE,
        seeds = [b"order", user.key().as_ref(), &order_id.to_le_bytes()],
        bump,
    )]
    pub pending_order: Account<'info, PendingOrder>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(order_id: u64)]
pub struct CloseOrder<'info> {
    // In production the platform co-signer settles; here the placing key doubles as it.
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
        mut,
        close = user,
        has_one = user,
        seeds = [b"order", user.key().as_ref(), &order_id.to_le_bytes()],
        bump = pending_order.bump,
    )]
    pub pending_order: Account<'info, PendingOrder>,
}

#[account]
pub struct PendingOrder {
    pub user: Pubkey,
    pub order_id: u64,
    pub ticker: String,
    pub usdc_amount: u64,
    pub created_at: i64,
    pub bump: u8,
}

impl PendingOrder {
    pub const SPACE: usize = 8 + 32 + 8 + (4 + MAX_TICKER_LEN) + 8 + 8 + 1;
}

#[event]
pub struct BuyOrderCreated {
    pub user: Pubkey,
    pub order_id: u64,
    pub ticker: String,
    pub usdc_amount: u64,
}

#[event]
pub struct BuyOrderFulfilled {
    pub user: Pubkey,
    pub order_id: u64,
    pub actual_usdc: u64,
}

#[event]
pub struct BuyOrderRefunded {
    pub user: Pubkey,
    pub order_id: u64,
}

#[error_code]
pub enum OrdersLiteError {
    #[msg("Ticker exceeds 8 characters")]
    TickerTooLong,
    #[msg("Order amount must be greater than zero")]
    ZeroAmount,
}
