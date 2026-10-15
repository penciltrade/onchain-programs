# Price trades with the Phoenix mark

An oracle supplies external price data. The mark is the oracle's reference trading price. Fills and busts use the Phoenix Perps mark through Hawkeye's readonly `view_bbo` view. A fill executes an admitted trading order. A bust forfeits margin and closes the position. The adapter pins the executable oracle program and configured account list.

The view returns 64 bytes. Those bytes contain best bid and ask ticks, mark and index ticks, and the mark and index update slots. A tick is one configured price increment. The adapter converts ticks to micro-USDC with the market's tick size. One micro-USDC equals one millionth of a USDC. A slot is a scheduled Solana block opportunity.

Phoenix documents a mark built from adjusted spot, book, and external perpetual components. PENCILTRADE consumes that mark without independently making sure that those components are correct. Earlier versions used the midpoint between best bid and ask. The deployed v4 program ignores those quotes for fills and busts. This change does not prove resistance to manipulation or oracle-lag arbitrage. Oracle-lag arbitrage exploits delayed price updates.

Best bid and ask remain visible for display. A crossed, locked, one-sided, or wide Phoenix book does not block execution.

## Fill within the admission window

An intent is an authorized action awaiting execution. Each admitted intent records its admission slot. With a nonzero delay, a fill requires a mark update slot inside this inclusive window:

```
[admitted_slot + fill_delay_slots, admitted_slot + fill_delay_slots + fill_window_slots]
```

- An earlier accepted mark defers the intent and preserves FIFO order.
- An accepted mark inside the window permits execution when the other requirements hold.
- A later accepted mark ends the intent with outcome 6 and restores reserved cash or claim margin.
- A rejected quote defers the intent until the window opens. From that point, rejection also ends it with outcome 6.

FIFO means first in, first out. The window limits the mark's update slot, rather than the execution slot. A mark from inside the window can execute later while fresh. The drainer's timing can select among available valid observations. The program records the newest consumed mark slot and rejects older marks. These rules do not prove the absence of oracle-lag arbitrage.

Local fixtures can set `fill_delay_slots = 0` to bypass the window comparison. Devnet and mainnet require a delay of 1–25 slots, equal across all markets. The window allows 1–50 slots. Initialization supplies both parameters. Before Seal, the loader authority can configure markets, but the common fill delay stays fixed. Sealed configuration cannot change.

## Reuse the first observation in each slot

V4 caches the first committed Hawkeye observation for each market and execution slot. A cache stores an observation for later reuse. Later instructions and transactions reuse it without another CPI. A cross-program invocation, or CPI, calls another Solana program. An upstream update within that slot cannot replace the snapshot. The mark and index source slots still determine freshness and fill eligibility.

A rejected quote can occupy the cache when Drain commits a refund or deferral. A failed transaction reverses its cache writes and can repeat the CPI attempt. The [cache rules](oracle-slot-cache.md) specify the exact cap and validation scope.

## Apply the price guards

The program accepts a quote only when all these conditions hold:

- The mark and index are nonzero.
- Neither update slot is in the future.
- Both update slots are at most `max_age_slots` old.
- The mark is no older than the last mark that this program consumed.
- The mark is within `max_deviation_bps` of the index.
- Outside local fixtures, the mark is within 30% of this program's last accepted mark for that market.

Mark and index share the same upstream program. An upstream change of price scale can move both together and pass the deviation guard. The final guard therefore compares the mark with this program's own record. It also permanently rejects a genuine move above 30% between consecutive checkpoints for one market. A checkpoint records a price that the program accepts. The market then ends through [wind-down](11-recovery.md) at its last accepted mark.

Anyone can submit checkpoints. The keeper attempts renewal after 60 seconds without an advancing checkpoint by default. The permanent rejection therefore requires a 30% move within a gap between checkpoints.

An unpriceably large mark, changed owner of a pinned oracle account, or malformed return data produces the same rejection, error 100. These conditions do not fail the transaction by themselves. Rejection blocks liquidation and `CheckpointOracle`. At the queue head, it defers an intent until its window opens, then ends it. Missing oracle accounts defer Drain. A valid mark that differs from the mark already consumed for that slot also defers Drain.

Other account or CPI errors can fail the transaction. With the required records, expired heads can clear without an oracle read. `max_spread_bps` remains in the account layout but does not gate execution. Side-presence flags also do not gate execution.

## Require a recent checkpoint for admission

Outside local fixtures, `Enqueue` and `EnqueueExit` require an accepted checkpoint from the last 120 seconds. Without it, they fail with error 100. A fill, liquidation, or `CheckpointOracle` from any signer supplies such a checkpoint. If the oracle stops answering, new intents stop two minutes later. This prevents unfillable orders from entering the shared FIFO. Existing positions remain open until the oracle recovers or the market winds down.

The keeper attempts renewal after 60 seconds without an advancing checkpoint by default. Trading can advance checkpoints sooner. The relayer declines preparation or co-signing when less than 30 seconds of checkpoint allowance remain. It requests a checkpoint itself so that a retry succeeds.

## Choose deployment parameters

| Parameter | Mainnet initialized value | Meaning |
| --- | --- | --- |
| `fill_delay_slots` | 10 (~4 s) | Delay after admission before a mark can fill. Every market uses the same delay. |
| `fill_window_slots` | 50 (~20 s) | Window during which a mark update can qualify for a fill. |
| `max_age_slots` | 30 (~12 s) | Maximum age of the mark and index. |
| `max_deviation_bps` | 300 | Maximum gap between mark and index. |

The deployed mainnet markets use these values. `artifacts/mainnet.json` records their oracle accounts, tick conversions, and multipliers.
Earlier deployment reports retain their recorded parameters. Approximate seconds assume 400 ms slots. Slot duration varies.

Three fixed constants also apply. Marks must remain within 3,000 bps of the last accepted mark. Admission requires a checkpoint no older than 120 seconds. Position notional cannot exceed $10,000,000. Notional is margin multiplied by leverage. Before Seal, the loader authority can change market parameters with `scripts/configure-market.mjs`.

1. Before Seal, measure mark lag against major spot venues.
2. Measure mark behavior when trades consume Phoenix's book.
3. Set the fill delay above the measured lag.

The repository's 15-minute sample covers calm conditions. It cannot establish either measurement for other conditions.

## Liquidate at the current mark

A position busts when the guarded mark alone crosses its trigger. Earlier versions required the midpoint, mark, and index to cross. Liquidation uses the current guarded mark without an admission window. Any signer can submit it. The result depends on the accepted current observation. The program does not reconstruct historical crossings.

## Use the devnet stub

Phoenix Hawkeye does not run on devnet. A separate stub oracle answers the same `view_bbo` call with the same 64-byte layout. It reads one quote account per market, updated by its authority. Devnet requires the configured stub to own every oracle account. Mainnet refuses the stub, and devnet refuses the real Hawkeye program.

The devnet feeder, `services/oracle-feeder.mjs`, writes these accounts. It can generate a random walk. It can also mirror the mainnet Hawkeye mark, index, and best quotes through read-only simulation. Devnet prices remain test inputs. They are not executable Phoenix prices.

## Interpret the evidence

A short, calm sample cannot prove resistance to a motivated attacker or liquidation safety in all market conditions. It cannot measure the cost of moving the mark's external components.

Before Seal, make sure that the following match a mainnet fork:

- The Hawkeye program identity and account lists.
- The tick conversion, tick size × 10^base-lot decimals.
- The return-data provenance and compute budget.

Collect longer observation windows that include stressed periods.

Sources include [Phoenix onchain programs](https://docs.phoenix.trade/sdk/on-chain-programs), [mark-price construction](https://docs.phoenix.trade/phoenix/margin-and-risk/mark-price), and the [official Rust SDK](https://github.com/Ellipsis-Labs/rise-public).

[Recovery](11-recovery.md) specifies permanent market wind-down, frozen-mark settlement, reservation refunds, timed public admission, and independent custody access. These termination rules belong to v4. Claims without full funding retain their existing repayment limits.
