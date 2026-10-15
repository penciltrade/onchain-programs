# Open and close positions

Margin is the USDC value committed to one position. Leverage multiplies that value to determine price exposure.
A 100 USDC margin at 200× creates 20,000 USDC of notional exposure.
The maximum notional is 10M USDC per position. The minimum margin is 1 USDC.

You can fund margin with available USDC or a specific unpaid profit claim.
A claim-funded position uses debt rather than a new USDC deposit.
The [balance guide](04-accounting.md) explains the different settlement rules for those sources.

## Choose the trade

1. Select BTC, ETH, or SOL.
2. Select long or short.
3. Choose the margin source and amount.
4. Choose leverage within the displayed limit.
5. Review the estimated outcome before signing.

The program fixes margin, leverage, direction, and entry price when the position opens.
You cannot add margin, change leverage, or close only part of that position.
You can open another independent position. Positions have no funding payment or time limit.

## Settle a gain

The program applies a nonlinear adjustment to a favorable price move.
The first 0.2 basis points of the move receive no gain. One basis point equals 0.01%.
The remaining gain uses the market's position multiplier, then a 2% fee reduces the adjusted gain.
This fee applies to adjusted profit, not position notional.

For readers who want the calculation, let `m` be the favorable price change divided by the entry price:

```text
m' = max(m - 0.00002, 0)
scale = 0.9 / (1 + 1/(15000*m') + 0.1/(m'*positionMultiplier))
adjustedGain = margin * leverage * m' * scale
traderProfit = adjustedGain - floor(adjustedGain / 50)
```

Gain is zero when `m'` is zero. The BTC multiplier is 814.598, the ETH multiplier is 483.979, and the SOL multiplier is 250.
The program calculates in integer units and rounds down. One USDC unit in its accounting equals one millionth of a USDC.
The website's estimates can differ slightly from the final integer calculation.

Larger favorable moves retain a larger fraction of raw gain. The gain adjustment is not a fixed payout percentage.
Economic profit has no fixed cap, but immediate USDC payment depends on available house funds and older claims.

## Settle a loss

An adverse move produces a raw loss of `margin × leverage × adverseMove`, limited to the position's margin.
No gain adjustment reduces losses. A voluntary cash-funded close returns the margin that remains after the loss.
Any eligible fee allocation from that loss comes from the house's gain. It does not add to your trading loss.

## Understand liquidation

A hard bust closes the position and forfeits its entire margin. Its approximate price thresholds are:

```text
long bust  = entry * (1 - 1/leverage + 0.0005)
short bust = entry * (1 + 1/leverage - 0.0005)
```

At 200×, a 0.45% adverse move reaches the threshold.
A position can retain value immediately before that threshold and lose all its margin once a valid liquidation occurs.
Anyone can submit a liquidation. It uses the current accepted oracle mark rather than historical price crossings.

Closing is also an order. Submission does not guarantee execution before a liquidation or adverse price move.
Check the position and its final outcome after the close fills.
The [price guide](06-oracle.md) explains which prices can authorize settlement.
