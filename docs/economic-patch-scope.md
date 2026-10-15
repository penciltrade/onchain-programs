# Define the neutral extraction patch

A delta neutral strategy limits combined price sensitivity. The source fix targets funded USDC extraction through settlement order and staking rewards. The threat model includes traders, stakers, the fee recipient, and cooperating wallets. An external hedge can reduce price exposure. Wallet identity therefore does not determine economic neutrality.

## Measure the complete strategy

1. Measure coalition USDC receipts minus contributions across the complete strategy.
2. Include deposits, returned margin, trading PnL, fees, staking rewards, and external hedge costs.
3. Exclude returned principal from profit.
4. Report unpaid claims and PENCIL credits separately.
5. After all legs settle, compare net house equity, team receipts, and coalition receipts.
6. Record residual price sensitivity throughout execution.

A coalition consists of participants who coordinate their actions. PnL means profit or loss. A claim is an owner's unpaid settlement amount. Gross treasury fees do not establish a positive net treasury contribution. Opposite notionals alone do not prove neutrality because winning payouts are nonlinear and losses stop at margin. Notional is margin multiplied by leverage.

## Correct the allocation source

The marginal staker share starts at 8%. It rises linearly from $100k to $1M and remains 99% above $1M. The independent 2% trading fee remains. The curve applies to distributable equity after the reserve for winning exposure. Winning exposure covers possible payouts to open winners.

Each market stores global long exposure units and short notional. Storage updates these counters from changes in loaded records. A caller cannot omit a winner to remove its reserve. Positions funded with cash or debt both contribute.

Short winning profit has a bound of 90% of notional. Long winning profit has a bound of 90% of price times notional divided by entry. The long reserve uses 130% of the last accepted mark. The mark is the oracle's reference trading price. This covers the largest next accepted increase. The bound omits the entry deduction, so it retains more cash than current winning PnL requires.

A missing long mark blocks distribution. Cash losses add net eligible gain to `lp_pending_profit`. Winning payouts and fees consume that budget. Profit sharing requires distributable equity above `lp_highwater` and cannot exceed the funded budget. A winning close can release profit deferred earlier.

A waiting payout queue, no active stake, or an ineligible loss clears the unallocated budget. An IOU is an unpaid debt obligation. Destroyed IOUs cannot supply distribution cash. Stopped-market settlements take no fee and share no LP profit. LP refers to liquidity in the house. Terminal residual distribution requires every market to stop and every position and debt to settle.

Deferred distributions belong to holders active at release. Already assigned fees retain their existing ownership.

## Test strategies

| Strategy | Required result |
| --- | --- |
| Opposite positions below the bust boundary | Both price directions and both settlement orders preserve house equity. |
| Coalition with all stake | Rewards remain below the trader's net cash cost. |
| Coalition of staker and team | Cash reconciliation leaves the retained house profit. |
| Split settlements | Allocation differences remain within declared atom rounding. |
| Equity recovery | Regained equity does not distribute the same profit again. |
| Loss funded with debt | Destroyed claims do not create distribution cash. |
| Omitted winner | Global exposure still blocks sharing of gross profit from the losing leg. |
| No active stake | Ineligible profit cannot become a later export of capital. |
| Stopped market | Settlement order preserves payout priority and funded cash identity. |
| Oracle/cache timing | Original source slots and the first committed snapshot govern execution. |

A bust forfeits margin and closes the position. An atom is the smallest token unit. A cache stores an observation for later reuse. Native regressions cover both directions, both close orders, seven equity levels, and 1, 40, or 400 pairs. They include 3, 10, and 30 bp moves at 100x, plus larger moves below bust. A basis point, or bp, equals 0.01%.

The compiled validator suite tests settlement of the losing leg first and actual SPL custody.

## Bound the conclusion

The reserve bounds a current or next accepted winning payout. It does not cover unlimited future mark increases. Price gaps across a bust boundary can invalidate the simple loss bound for opposing legs. A surviving gap strategy needs measurements of price sensitivity, hedge cost, timing, and net funded cash extraction. An economical external hedge can keep it within patch scope.

PENCIL issuance alone does not demonstrate a USDC drain. A blocker requires a complete strategy that converts issuance into uncompensated funded cash extraction. Directional exposure does not excuse custody, arithmetic, signature, or funding failures. Zero initial house liquidity, uncapped directional profit, and gross-loss emissions remain protocol choices.

Bind every result to candidate source and SBF fingerprints.

SBF is Solana's compiled program format. Earlier devnet throughput does not establish this settlement code or cache policy. Independent strategy review, live-oracle testing, and sustained load remain acceptance gates.
