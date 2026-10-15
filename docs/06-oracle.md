# Understand prices and execution

Trades and liquidations use the Phoenix Perps mark through the Hawkeye oracle on Solana.
An oracle supplies external price data. The mark is its reference trading price.
The program does not use the best bid, best ask, or a chart midpoint as the settlement price.

## Read the price shown on the website

The displayed price and estimated profit describe the latest available information.
Your final entry or close price comes from the observation the program accepts for execution.
A delay between signing and execution can change the result.
The website's PnL estimates also differ slightly from the program's integer rounding.

## Understand the price window

An admitted order waits ten slots before its fill window begins. The following 50 slots form the price window.
A slot is a Solana block opportunity. Slot timing varies with the network.
The mark must satisfy the order's timing, limit, expiry, and oracle rules.

The program checks mark and index freshness, price deviation, and source ordering.
Mark and index must be no more than 30 slots old and within 3% of each other.
The next accepted checkpoint cannot move more than 30% from the preceding accepted mark.
A checkpoint records a price that the program accepts.

A wide, crossed, or one-sided orderbook does not by itself invalidate an otherwise valid mark.
The program relies on Phoenix's mark construction and Hawkeye's returned data.
It does not independently reconstruct every external component of that mark.

## Handle stale prices

New orders require a checkpoint from the previous 120 seconds.
If accepted checkpoints stop, new order admission stops after that allowance expires.
Existing positions remain open while the oracle is unavailable.
An outage can delay closes and liquidations and expose a position to a later adverse price.

Anyone can publish a valid advancing checkpoint. Normal fills and liquidations can also advance the checkpoint.
For each market, the first committed oracle observation fixes the program's price snapshot for that execution slot.

## Understand liquidation prices

A liquidation uses the current accepted mark.
The program does not replay a price excursion that recovered before an accepted liquidation observation.
Once a hard bust settles, the position cannot revive.

If a sealed market permanently stops, its positions settle against its last accepted frozen mark.
That mark can differ from the current external market price.
The [outage guide](11-recovery.md) explains this recovery path and its limits.
