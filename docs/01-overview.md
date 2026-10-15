# Start with Penciltrade

Penciltrade lets you trade BTC, ETH, and SOL price exposure with USDC on Solana.
A long gains from a price increase. A short gains from a price decrease.
The protocol records your position without buying, selling, or hedging the underlying asset.

Each position has isolated margin. Its trading loss cannot exceed the margin committed to that position.
You can choose leverage from 1× to 200×. The minimum margin is 1 USDC, and maximum notional is 10M USDC per position.
Notional means margin multiplied by leverage. Positions have no funding payment or fixed expiry.

## Make your first trade

1. Connect your Solana wallet.
2. Deposit native Solana USDC.
3. Choose a market, direction, margin, and leverage.
4. Review the order and authorize it in your wallet.
5. Wait for the order to fill before treating the position as open.

Your wallet needs SOL for transaction fees and account storage.
If wallet registration is required, use the registration action before opening a position.
The launch countdown controls when the website accepts new trading actions.

A submitted order is not a filled trade. Your position opens only after the program accepts an eligible oracle price.
The [order guide](03-queues.md) explains admission, execution, expiry, and payout priority.

## Understand your balances

Your deposit becomes your available USDC balance. Opening a position reserves its margin until execution and settlement.
The protocol uses shared funds, called the house, to pay trading profits.
Your deposit does not become a contribution to the house simply because you deposit it.

If the house cannot pay a profit immediately, the unpaid amount becomes a claim.
Claims have a repayment order, but repayment depends on future house funds.
The protocol does not guarantee that every profit claim will receive USDC.

Eligible realized losses earn PENCIL credits. You can mint those credits into tokens and stake the tokens for eligible USDC rewards.
The [PENCIL guide](05-ownership.md) explains emissions, staking, and the reward curves.

## Know the risks

High leverage can consume an entire position margin after a small adverse price move.
Liquidation forfeits the whole margin. A favorable trade receives the protocol's gain adjustment before its trading fee.
Price outages can delay execution and liquidation. A later execution price can differ from the price shown before signing.

Smart contract, oracle, wallet, and Solana failures can affect access to funds.
PENCIL has no guaranteed resale value, market liquidity, or redemption right against the USDC vault.
Review the [trading rules](02-trading.md), [balances](04-accounting.md), and [program permissions](07-programs.md) before committing funds.
