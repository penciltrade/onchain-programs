# Follow the trading rules

Margin is the value committed to a position. Leverage multiplies that value to determine price exposure. Long exposure gains from rising prices. Short exposure gains from falling prices.

## Open and manage a position

1. Choose a configured market.
2. Choose long or short exposure.
3. Choose your margin source and amount.
4. Choose leverage within the current maximum of at most 200×.

Your margin can use funded available USDC or one of your unpaid claims. A claim is an owner's unpaid settlement amount. An accepted open reserves the chosen source until execution. The relayer co-signs admission, and your wallet or scoped delegate authorizes the trade.

A scoped delegate is a key with limited authority. The program reads the oracle at execution. An oracle supplies external price data. Each position fixes its market, direction, margin, leverage, entry price, and economic parameter version. You cannot change margin or leverage, or partially close a position. You can open another independent position.

Positions have no funding payment and no time limit. Notional is margin multiplied by leverage. A $100 margin at 200× creates $20,000 of synthetic exposure. The house records this exposure in its ledger. The program does not buy $20,000 on Phoenix.

The notional limit is $10 million per position, with a $1 minimum margin. An owner can hold several positions. No aggregate credit ceiling or profit cap applies.

## Settle a winning position

The gain haircut reduces the profit from a favorable move. The deadband excludes the first 0.2 bp of that move. A basis point, or bp, equals 0.01%. In the calculation, `m = abs(exit-entry)/entry`. After the deadband, `m' = max(m - 0.00002, 0)`.

The original gain scale is:

```
scale = 0.9 / (1 + 1/(15000*m') + 0.1/(m'*positionMultiplier))
adjustedGain = margin * leverage * m' * scale
winFee = adjustedGain * 0.02
traderProfit = adjustedGain - winFee
```

BTC uses positionMultiplier 814.598. ETH uses 483.979. The gain is zero at zero effective move. The program uses fixed-point calculations and rounds down to micro-USDC. One micro-USDC equals one millionth of a USDC. The multiplier depends on the market, so trade splitting does not remove the percentage haircut.

The equations describe the curve. The integer implementation first rounds raw effective gain down, then rounds adjusted gain down. The fee is `floor(adjustedGain / 50)`. The repository's `formal-verification/model.md` specifies exact rounding. Small trade splits can produce rounding differences.

For BTC, the trader receives approximately 20.95% of a raw 1 bp gain after the deadband and fee. The trader receives approximately 72.43% at 10 bp and 86.38% at 100 bp. ETH retains about 85.68% at a 100 bp move. These market calibrations do not provide a fixed 78% payout. Larger moves retain a higher percentage. Economic profit has no cap, but all arithmetic must fit the onchain integers.

A position funded with cash returns its principal to your available balance immediately. Principal is the position's original margin. If no older claims exist, house cash pays net profit. Any unpaid profit joins the FIFO claim queue. FIFO means first in, first out. A position funded with debt settles surviving principal and net profit at the new close priority.

The 2% winning fee always reduces the trader's gain. The fee becomes funded team and staker revenue only when the house can pay it after unpaid winners. An unfunded withheld fee creates no additional fee claim.

## Settle a losing position

Raw loss is `margin * leverage * adverseMove`, capped at margin. No gain haircut applies to losses. Positions funded with cash return margin minus loss as funded USDC. Lost cash becomes house cash. While claims remain, that cash serves their repayment before fees.

If the payout queue is empty, an eligible cash loss normally allocates 2% of the house's gain as a fee. This allocation does not increase the trader's loss. A loss on debt cancels that part of the IOU and creates no USDC. An IOU is an unpaid debt obligation. Surviving principal settles at a new claim priority. The house funds a fee on debt loss only when it can pay the surviving claim and fee after existing claims.

## Apply the hard-bust rule

A hard bust forfeits margin and closes the position. The approximate triggers are:

```
long bust  = entry * (1 - 1/leverage + 0.0005)
short bust = entry * (1 + 1/leverage - 0.0005)
```

At 200×, a 45 bp adverse move reaches the trigger. The theoretical move for zero equity is 50 bp. Immediately before bust, a $100 position can still have $10 of equity through the voluntary close calculation. Once a fresh guarded mark crosses the trigger, the trader forfeits the entire $100. This is the original hard-bust rule.

The mark is the oracle's reference trading price. Liquidation uses only the current guarded Phoenix mark. Bid and ask do not enter the bust calculation. The program trusts the configured oracle's mark construction. Voluntary closes use the mark from their fill window. A fill window limits the acceptable mark update slots.

If a price excursion recovers before an accepted liquidation checkpoint, the program does not replay the historical crossing. A checkpoint records a price that the program accepts. A settled bust cannot revive. A bust takes precedence over a voluntary close limit.

Anyone can submit a liquidation transaction. Oracle guards also apply to busts. Zero, future, stale, regressing, or excessively deviating mark/index data cannot authorize liquidation. A wide, crossed, or one-sided book does not block a valid mark. An oracle outage delays fills and busts. It does not guarantee the eventual price.

## Understand the cost of neutral trades

Equal long and short exposure loses value through the winning haircut when both legs settle normally. This cost does not prove that loss-based token issuance prevents farming. Farming creates trades to collect rewards. Debt principal has no cash funding, token resale value can exceed USDC cost, and full-margin busts can make the legs asymmetric.

PENCIL emissions reward gross eligible loss as an experiment. They do not prove net economic contribution.

[Recovery](11-recovery.md) specifies permanent market wind-down, frozen-mark settlement, reservation refunds, timed public admission, and independent custody access. These termination rules belong to v4. Claims without full funding retain their existing repayment limits.
