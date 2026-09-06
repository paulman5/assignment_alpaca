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
        ctx: Context<PlaceOrder>,
        order_id: u64,
        ticker: String,
        usdc_amount: u64,
    ) -> Result<()> {
        let user = ctx.accounts.user.key();
        init_order(ctx, order_id, ticker.clone(), usdc_amount, OrderSide::Buy)?;
        emit!(BuyOrderCreated {
            user,
            order_id,
            ticker,
            usdc_amount,
        });
        Ok(())
    }

    // Sells mirror buys: amount is in token units; production locks these in escrow.
    pub fn place_sell_order(
        ctx: Context<PlaceOrder>,
        order_id: u64,
        ticker: String,
        token_amount: u64,
    ) -> Result<()> {
        let user = ctx.accounts.user.key();
        init_order(ctx, order_id, ticker.clone(), token_amount, OrderSide::Sell)?;
        emit!(SellOrderCreated {
            user,
            order_id,
            ticker,
            token_amount,
        });
        Ok(())
    }

    pub fn fulfill_buy_order(
        ctx: Context<CloseOrder>,
        order_id: u64,
        actual_usdc: u64,
    ) -> Result<()> {
        require!(
            ctx.accounts.pending_order.side == OrderSide::Buy,
            OrdersLiteError::WrongOrderSide
        );
        emit!(BuyOrderFulfilled {
            user: ctx.accounts.user.key(),
            order_id,
            actual_usdc,
        });
        Ok(())
    }

    pub fn fulfill_sell_order(
        ctx: Context<CloseOrder>,
        order_id: u64,
        actual_usdc: u64,
    ) -> Result<()> {
        require!(
            ctx.accounts.pending_order.side == OrderSide::Sell,
            OrdersLiteError::WrongOrderSide
        );
        emit!(SellOrderFulfilled {
            user: ctx.accounts.user.key(),
            order_id,
            actual_usdc,
        });
        Ok(())
    }

    pub fn refund_buy_order(ctx: Context<CloseOrder>, order_id: u64) -> Result<()> {
        require!(
            ctx.accounts.pending_order.side == OrderSide::Buy,
            OrdersLiteError::WrongOrderSide
        );
        emit!(BuyOrderRefunded {
            user: ctx.accounts.user.key(),
            order_id,
        });
        Ok(())
    }

    pub fn refund_sell_order(ctx: Context<CloseOrder>, order_id: u64) -> Result<()> {
        require!(
            ctx.accounts.pending_order.side == OrderSide::Sell,
            OrdersLiteError::WrongOrderSide
        );
        emit!(SellOrderRefunded {
            user: ctx.accounts.user.key(),
            order_id,
        });
        Ok(())
    }
}

fn init_order(
    ctx: Context<PlaceOrder>,
    order_id: u64,
    ticker: String,
    amount: u64,
    side: OrderSide,
) -> Result<()> {
    require!(ticker.len() <= MAX_TICKER_LEN, OrdersLiteError::TickerTooLong);
    require!(amount > 0, OrdersLiteError::ZeroAmount);

    let order = &mut ctx.accounts.pending_order;
    order.user = ctx.accounts.user.key();
    order.order_id = order_id;
    order.side = side;
    order.ticker = ticker;
    order.amount = amount;
    order.created_at = Clock::get()?.unix_timestamp;
    order.bump = ctx.bumps.pending_order;
    Ok(())
}

pub const MAX_TICKER_LEN: usize = 8;

#[derive(Accounts)]
#[instruction(order_id: u64)]
pub struct PlaceOrder<'info> {
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

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[account]
pub struct PendingOrder {
    pub user: Pubkey,
    pub order_id: u64,
    pub side: OrderSide,
    // amount is USDC (6 dp) for buys, token units for sells
    pub ticker: String,
    pub amount: u64,
    pub created_at: i64,
    pub bump: u8,
}

impl PendingOrder {
    pub const SPACE: usize = 8 + 32 + 8 + 1 + (4 + MAX_TICKER_LEN) + 8 + 8 + 1;
}

#[event]
pub struct BuyOrderCreated {
    pub user: Pubkey,
    pub order_id: u64,
    pub ticker: String,
    pub usdc_amount: u64,
}

#[event]
pub struct SellOrderCreated {
    pub user: Pubkey,
    pub order_id: u64,
    pub ticker: String,
    pub token_amount: u64,
}

#[event]
pub struct BuyOrderFulfilled {
    pub user: Pubkey,
    pub order_id: u64,
    pub actual_usdc: u64,
}

#[event]
pub struct SellOrderFulfilled {
    pub user: Pubkey,
    pub order_id: u64,
    pub actual_usdc: u64,
}

#[event]
pub struct BuyOrderRefunded {
    pub user: Pubkey,
    pub order_id: u64,
}

#[event]
pub struct SellOrderRefunded {
    pub user: Pubkey,
    pub order_id: u64,
}

#[error_code]
pub enum OrdersLiteError {
    #[msg("Ticker exceeds 8 characters")]
    TickerTooLong,
    #[msg("Order amount must be greater than zero")]
    ZeroAmount,
    #[msg("Instruction side does not match the order's side")]
    WrongOrderSide,
}
