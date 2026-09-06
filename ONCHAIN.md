# orders-lite

The on-chain side of the Spout backend take-home. It is a stripped-down slice of our production orders program — the order lifecycle only. The KYC gating, oracle validation, and USDC/token escrows are removed and are **not part of the assessment in any form** — no identity checks, no compliance logic. It is **already deployed on devnet and you write no on-chain code**; your settlement worker talks to this program the way our real backend talks to the real one.

## Deployed program

| | |
|---|---|
| Program ID (devnet) | `DB2h5equ9Qp2qaaeKyL9sL6RD8LAhZzjiSUYgnaW3aeF` |
| IDL | [`idl/orders_lite.json`](idl/orders_lite.json) |
| Source | [`programs/orders-lite/src/lib.rs`](programs/orders-lite/src/lib.rs) (~150 lines, worth reading) |

## The lifecycle

- `place_buy_order(order_id, ticker, usdc_amount)` / `place_sell_order(order_id, ticker, token_amount)` — creates a `PendingOrder` PDA at seeds `["order", user, order_id_le]` (with a `side` field) and emits `BuyOrderCreated` / `SellOrderCreated`. Permissionless; sign with your own devnet keypair. `(user, order_id)` is the business key, shared across both sides — one counter per user, like production.
- `fulfill_buy_order(order_id, actual_usdc)` / `fulfill_sell_order(order_id, actual_usdc)` — closes the PDA, emits `BuyOrderFulfilled` / `SellOrderFulfilled`. This is your worker's settlement call. The instruction must match the order's side — a buy fulfill on a sell order fails with `WrongOrderSide`.
- `refund_buy_order(order_id)` / `refund_sell_order(order_id)` — closes the PDA, emits `BuyOrderRefunded` / `SellOrderRefunded`. The terminal for orders that can't fill (e.g. market closed).

A closed PDA is gone: retrying fulfill/refund fails with Anchor error 3012 `AccountNotInitialized`. **That is the on-chain "already settled" signal** — our production runbook literally says "if the PDA is already gone → already_settled, move on". Your retry logic must treat it that way, not as a failure.

In production the platform co-signer fulfills orders; here your keypair doubles as both the user placing and the platform settling.

## Getting started

```bash
npm install
solana-keygen new -o devnet-keypair.json   # if you don't have one
solana airdrop 2 -k devnet-keypair.json -u devnet
KEYPAIR=devnet-keypair.json npm run place-order -- AAPL 2500       # buy 2500 USDC of AAPL
KEYPAIR=devnet-keypair.json npm run place-order -- NVDA 3 sell     # sell 3 NVDA tokens
```

Events arrive as Anchor's `Program data:` base64 lines in the program logs — `logsSubscribe` (websocket) with mentions on the program ID, decode with the IDL (`scripts/place-order.mjs` shows the client setup; the event coder is `program.coder.events`). Websocket delivery is where your duplicate/reconnect handling becomes real.

Devnet RPC defaults to the public endpoint; set `RPC_URL` if you have your own.
