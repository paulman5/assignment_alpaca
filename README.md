# Spout Backend Technical Assignment 

## Context

Spout tokenizes US equities on Solana. A user buys `sAAPL` by placing an order on-chain (their USDC is escrowed); our backend detects the on-chain order, executes the real trade at a brokerage API, and once the trade fills, settles back on-chain (tokens minted to the buyer, or sale proceeds paid out). Sells are the mirror image.

The dangerous part of this system is the window between "real stock was bought/sold" and "chain has settled". A bug there means a user paid and received nothing, or we traded twice for one order. The backend's job is to make that window short, observable, and impossible to double-execute.

This assignment is a scaled-down version of that pipeline, and its on-chain side is real: we deployed **orders-lite**, a stripped slice of our production orders program, on Solana devnet — its source, IDL, and example client are in this repo. **You write no on-chain code** — your worker consumes its events and sends transactions to it, the way our backend talks to the real program. No prior Solana experience is assumed; [ONCHAIN.md](ONCHAIN.md) covers everything you need.

## Ground rules

- **Deadline: one week** from receiving access.
- Stack: **TypeScript + Node.js + Postgres** (we run NestJS + TypeORM, but any framework or none is fine). No message broker — if you need a queue, build it on Postgres.
- Everything runs locally with `docker compose up` + one or two npm scripts, plus a devnet keypair (`solana-keygen new` + `solana airdrop` — free test SOL, instructions in [ONCHAIN.md](ONCHAIN.md)).

## Build the pipeline

You are building three small pieces: an **order event listener** on the devnet program, a **broker integration** (the real Alpaca sandbox, plus a mock with the same interface for failure testing), and the thing we actually evaluate — the **settlement worker** between them.

### 1. Order event listener (real, on devnet)

The `orders-lite` program (`DB2h5equ9Qp2qaaeKyL9sL6RD8LAhZzjiSUYgnaW3aeF`, devnet) emits `BuyOrderCreated { user, order_id, ticker, usdc_amount }` when an order is placed — this repo has the IDL and a `place-order` script, so you generate your own orders with your own keypair. Your listener subscribes to the program's logs over websocket (`logsSubscribe`), decodes the events, and feeds your pipeline.

Treat delivery as **at-least-once and unordered** — websockets drop, reconnects replay, and your listener must also survive events it has already seen. `order_id` is a per-user counter, so `(user, order_id)` is the business key; the transaction signature is unique per event. Persist raw events as you capture them: several scenarios below are demonstrated by replaying them through your pipeline.

### 2. Broker integration

Your worker places its trades at the **real Alpaca Broker API sandbox** — credentials, auth, and docs are in the Appendix. This is a core requirement, not a stretch goal: one order must demonstrably travel devnet event → sandbox trade → devnet settlement (scenario G below).

The sandbox can't produce failures on demand, so you also implement a **mock broker** with the same interface, config-switched with the real one; the failure scenarios run against it:

- `POST /orders` with `{ client_order_id, symbol, notional }` → `202 { broker_order_id, status: "accepted" }`. Fills happen asynchronously a few seconds later.
- **Same `client_order_id` twice → `409`** with the original order. This is your exactly-once backstop; use it like a real broker's.
- `GET /orders/:broker_order_id` → current status (`accepted | filled | rejected`).
- `GET /orders?client_order_id=...` → lookup for recovery.
- Simulated failures, controlled by env flags or a seed: random 500s, slow responses (2–10 s), and a "market closed" rejection.

The endpoints that matter for this pipeline (your mock mirrors these; sandbox only — never the live environment, and keep credentials out of your submission repo):

| Endpoint | Role in the pipeline |
|---|---|
| `POST https://broker-api.sandbox.alpaca.markets/v1/trading/accounts/{account_id}/orders` | Place the trade (`client_order_id`, `notional` or `qty`; duplicate `client_order_id` → `409`) |
| `GET https://broker-api.sandbox.alpaca.markets/v1/trading/accounts/{account_id}/orders/{order_id}` | Poll order status |
| `GET https://broker-api.sandbox.alpaca.markets/v1/trading/accounts/{account_id}/orders:by_client_order_id?client_order_id={client_order_id}` | Crash-recovery lookup |
| `DELETE https://broker-api.sandbox.alpaca.markets/v1/trading/accounts/{account_id}/orders/{order_id}` | Cancel before fill |
| `GET https://broker-api.sandbox.alpaca.markets/v1/events/trades` | SSE stream of fill events (the polling alternative) |
| `GET https://broker-api.sandbox.alpaca.markets/v1/trading/accounts/{account_id}/account` | Account balance state for reconciliation |
| `GET https://broker-api.sandbox.alpaca.markets/v1/trading/accounts/{account_id}/positions` | Open positions for reconciliation |

Your broker client is one interface with two implementations — real sandbox and mock. That separation is part of what we grade.

### 3. Settlement worker (the actual assignment)

Consumes order events and drives each order through: `recorded → broker_placed → filled → settled`, calling the broker (real sandbox or mock, behind your interface) to place, polling for fills, then settling **for real on devnet**: `fulfill_buy_order(order_id, actual_usdc)` closes the order's PDA and emits `BuyOrderFulfilled`; orders that can't fill terminate via `refund_buy_order`. A closed PDA makes any retry fail with Anchor error 3012 `AccountNotInitialized` — that is the on-chain "already settled" signal, and your retry logic must treat it as success, not failure.

Hard requirements:

1. **Idempotency end-to-end.** Duplicate or replayed events must never cause a second broker order or a second settlement. Dedupe must survive a process restart (i.e., live in Postgres, not in memory).
2. **Crash safety.** `kill -9` your worker at the worst possible moment — after the broker accepted but before you wrote that to the DB — and on restart nothing is trade-duplicated and the order still completes. (Hint: this is what the `client_order_id` recovery lookup exists for.)
3. **An auditable ledger.** We want to be able to reconstruct the history of every order from the database: what happened, when, in what order. Design the schema; explain it in the README.
4. **Concurrent workers.** Two instances of the worker running against the same database must not double-process. Any mechanism is fine (row claims with leases, `FOR UPDATE SKIP LOCKED`, advisory locks) — explain your choice.
5. **Retries with an end state.** Retry only *transient* failures — HTTP 5xx, timeouts, connection errors, rate limits — with exponential backoff (e.g. 1 s doubling up to 60 s, max ~10 attempts). Permanent rejections (market closed, invalid symbol) are never retried. When retries are exhausted, revert deliberately: first confirm via the `by_client_order_id` lookup that no broker trade actually exists — if none does, call `refund_buy_order` on-chain, mark the order in a terminal failed state, and alert (a log line is fine). If a broker trade **filled**, never revert or abandon it — that is a user who paid real money: dead-letter it loudly for a human.
6. **Reconciliation.** A job (cron-style or on-demand script) that compares broker state vs. your DB vs. chain state (does the `PendingOrder` PDA still exist?) and repairs drift — e.g., an order the broker filled but your DB thinks is still `broker_placed` because you missed the poll.

### Scenarios your README must show how to run

Provide a way (script, seed flag, or manual steps) to demonstrate each:

- **A.** Same event replayed 3× through the pipeline → exactly one broker order, one settlement.
- **B.** Worker killed between broker-accept and DB write → restart → no duplicate trade, order completes.
- **C.** Broker returns 500s for 30 s → worker retries with backoff and the order completes once the outage ends — no refund, no dead-letter, because the outage ends within the retry policy.
- **D.** Settlement transaction sent, confirmation response lost (timeout) but the transaction landed → retry hits `AccountNotInitialized` → recorded as settled, no double-send loop, DB converges.
- **E.** Market closed rejection → `refund_buy_order` on-chain, distinct terminal state, not the DLQ retry loop.
- **F.** Reconciliation over drifted state → a fill your poller missed is found via the broker, and an order your DB thinks is pending but whose PDA is gone is repaired.
- **G.** One order end-to-end against the **real Alpaca sandbox**: devnet `BuyOrderCreated` → sandbox order placed → fill detected → `fulfill_buy_order` settles on devnet.

## Submission

- Private GitHub repo, invite us; or a zip.
- README: how to run, how to trigger scenarios A–G, schema rationale, failure-mode table (what fails → what happens → who notices), what you cut, what you'd do with two more weeks.
- Rough hours spent (no judgment — it calibrates our review).

## What we evaluate

Correct idempotency and crash recovery first; the integrations — devnet program and Alpaca sandbox — (event handling, retry semantics, commitment levels) and schema/ledger design second; failure handling and observability third; code clarity and tests fourth. A smaller solution that nails A–G beats a bigger one that hand-waves them. The next interview round starts from your code: you'll walk us through it and we'll extend the design together.

## Appendix — what you need to start

**Alpaca Broker API (sandbox):**

| | |
|---|---|
| API route | `https://broker-api.sandbox.alpaca.markets` |
| Client ID | `CKEU2CYQBMQXZV4TD3452XTO3R` |
| Client Secret | `GBzHJoTbySfU24CWUuVWpgbYyAEvJ8AW9j7kafx1poiT` |
| Auth | OAuth2 client credentials → Bearer token (Basic auth returns 401 on this key) |
| API docs | https://docs.alpaca.markets/us/docs/getting-started |

Token exchange (tokens are valid ~15 minutes — cache and refresh):

```bash
curl -X POST 'https://authx.sandbox.alpaca.markets/v1/oauth2/token' \
  -H 'Content-Type: application/x-www-form-urlencoded' \
  -d 'grant_type=client_credentials&client_id=CKEU2CYQBMQXZV4TD3452XTO3R&client_secret=GBzHJoTbySfU24CWUuVWpgbYyAEvJ8AW9j7kafx1poiT'
# then: Authorization: Bearer <access_token>
```

These are **sandbox-only** credentials shared for this assignment (no real money anywhere behind them) and will be rotated afterwards. Load them from env in your code and keep them out of your submission repo.

**USDC on devnet (for the off-ramp leg):**

| | |
|---|---|
| USDC mint (devnet) | `4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU` |
| Faucet | https://faucet.circle.com/ — select **Solana Devnet**, paste your wallet address |
| Alpaca off-ramp wallet (owner) | `7YBM7UQitFS9rJR2YEfnXmGEJREfstKfRBjZRRyatqo2` |
| Approved (whitelisted) USDC deposit ATA | `ADZRYU7F5t4vzhBCihr9faNEYPbGAHYCiC8c3mkZKR2d` |

Get test USDC from the faucet into your devnet wallet. When your pipeline off-ramps order USDC to Alpaca, the whitelisted deposit ATA above is the **only approved destination** — in production the program pins the destination's owner to the off-ramp wallet, so treat any other address as a rejected transfer.

**On-chain (devnet):** [ONCHAIN.md](ONCHAIN.md) has everything — program ID `DB2h5equ9Qp2qaaeKyL9sL6RD8LAhZzjiSUYgnaW3aeF`, the IDL, the `place-order` script, and keypair/airdrop setup. There is also a mock security-token mint:

| | |
|---|---|
| Token | **smockAAPL** |
| Mint address (devnet) | `Do28sRirHZUszT37DxXoRbBrFZtB6Au4eVyQigLyBzwL` |
| Decimals | **9** — Alpaca expects 9-decimal amounts on any order, so handle token quantities at 9 dp |

Mint test tokens to yourself with `npm run mint-mock -- <amount>` to mirror what production mints on fulfillment.
