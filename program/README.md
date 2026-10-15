# PENCILTRADE v4 Solana programs

This workspace contains the native trading program and a separate test oracle.
The trading program settles synthetic exposure against one shared house.
It does not place Phoenix orders, hedge positions, or collect funding.
The house starts with zero cash. Unpaid profits become unsecured FIFO claims.

## Source map

| File | Responsibility |
| --- | --- |
| `src/types.rs` | Borsh instruction, root, and record schemas. |
| `src/math.rs` | Integer PnL, hard-bust threshold, emission curve, and 256-bit intermediates. |
| `src/ledger.rs` | Funded balances, debt, fees, staking rewards, and FIFO repayment. |
| `src/storage.rs` | Record PDAs, subset accounting, rent recycling, rent float, and reclaim. |
| `src/lib.rs` | Account authentication, oracle CPI, admission, execution, custody, immutable authority, and recovery. |
| `mock-oracle/src/lib.rs` | Authority-controlled quote accounts and the Hawkeye-compatible view. |

`../protocol-abi.json` describes the generated ABI. `../sdk/` implements the JavaScript ABI.
`../docs/07-programs.md` lists instruction tags and their account requirements.
`../formal-verification/` defines the review scope and properties. No property has proof evidence yet.

## Accounts and units

The root PDA uses `[b"state"]`. It contains configuration, counters, aggregate liabilities, markets, and recent outcomes.
Users, positions, claims, intents, and proposals occupy separate record PDAs.
The runtime loads only the supplied records.

Record seeds contain the lowercase kind and the owner pubkey or little-endian `u64` identifier.
Each record contains a kind byte, a 32-byte rent payer, and a Borsh payload.
The root contains a four-byte payload length followed by Borsh data.
The root starts at 8 KiB and can grow in 4 KiB increments to 64 KiB.

USDC and PENCIL use six-decimal SPL Token amounts. Prices use micro-USD.
The root PDA controls both vaults and the PENCIL mint authority.
PENCIL starts with zero supply and has no freeze authority.
Sides are `1` for long and `-1` for short.
Tickets, record identifiers, and market sequences start at 1. User nonces start at 0.

**Use fresh v4 state and a new program ID. There is no funded-state migration from earlier versions.**

## Cash and debt

The funded-accounting requirement is:

```text
USDC vault >= available balances + pending cash margins + open cash margins
             + house cash + funded team fees + funded staker fees
debt = payable claims + pending debt margins + open debt margins
tracked LP = house cash - debt
```

Each aggregate liability field uses `u64`. The debt sum uses `u128`. Tracked LP uses `i128`.
Subset updates subtract old loaded liabilities and add new loaded liabilities.
Omitted records remain in the aggregate totals.
Unsolicited vault transfers remain uncredited excess. They do not create house cash or user balances.

A cash close returns surviving margin directly to available USDC.
Cash winners receive profit from house cash or an owner-bound claim.
Cash losses create house cash. Debt losses cancel liabilities without creating USDC.
Harvest converts genuine house cash into the oldest claimant's available balance.
It permits partial repayment and does not transfer tokens to the claimant's wallet.

A pending debt open reserves part of an existing claim.
Its claim record remains a FIFO barrier until execution or terminal release.
Release restores the amount in the original record. Execution surrenders the reserved portion's old priority.
Settlement pays or appends surviving debt principal and profit at the new settlement order.
Claim IDs define repayment order. The `priority` field records the settlement ticket, which is zero for public liquidations.

## Position arithmetic

Admission permits 1–200x leverage, subject to the current maximum.
The minimum margin is $1. The maximum position notional is $10 million.
Positions preserve margin, side, leverage, entry, and the multiplier stored at execution.
Positions have no funding, duration limit, partial close, margin edits, or economic gain cap.

Loss is `floor(margin * leverage * abs(exit-entry) / entry)`, capped at margin.
Favorable moves subtract a 0.2 bp deadband before the nonlinear haircut.
The exact integer specification appears in `../formal-verification/model.md`.
The winning fee is `floor(adjusted_gain / 50)`. Net profit is the remaining amount.
The implementation uses custom 256-bit intermediates and rejects unrepresentable results.

A hard bust occurs at an adverse move of `1/leverage - 0.0005` or more.
At 200x this boundary is 0.45%. A bust destroys the entire margin.
The current accepted mark determines the result. No historical crossing is replayed.
A bust takes precedence over a voluntary close limit.
Public liquidations can settle positions between admitted intents.

## Oracle and fill timing

Mainnet mode requires canonical USDC `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v`.
It pins Hawkeye `RiSeVw3ZjNfsaXPRb4mgaqYaEEt41pNNJoDvVh7pgQj`.
It requires configured Phoenix accounts under `EtrnLzgbS7nMMy5fbD42kXiUzGg8XQzJ972Xtk1cjWih`.
Devnet requires a different oracle program and accounts owned by that program.
Local fixtures relax oracle account ownership requirements.
Deployment mode comes from initialization flags, not a runtime cluster identity check.

The adapter calls readonly `view_bbo`.
It requires the configured return-data program, 64-byte length, magic, and version 1.
Execution uses the mark. Mark and index must be nonzero, fresh, nonfuture, and within the configured deviation.
The mark slot cannot precede the last consumed mark slot for that market.
Bid, ask, and side-presence flags do not gate execution. `max_spread_bps` remains an unused ABI field.

For nonzero fill delay, a successful intent uses a mark updated within this inclusive interval:

```text
[admitted_slot + fill_delay_slots,
 admitted_slot + fill_delay_slots + fill_window_slots]
```

An earlier mark defers the head. An accepted later mark ends the intent with outcome 6.
Invalid mark/index data defers until another valid observation or expiry.
If an in-window mark remains fresh, it can still execute later.
The runtime does not select a unique mark within the window or enforce a wall-clock fill deadline.
Public liquidations use the current guarded mark without the admission window.

Fill delay ranges from 1 to 25 slots outside local mode. The window ranges from 1 to 50 slots.
Local mode permits zero delay and bypasses the window comparison in that case.
The devnet bootstrap defaults are 6 slots, 24 slots, 20-slot reference age, and 50 bp deviation.
The SDK supplies mainnet starter defaults: 10-slot delay, 50-slot window, 30-slot freshness, and 300 bp mark/index deviation.
Initialization records explicit parameters. Seal freezes them. See `../docs/immutable-release.md` for the complete profile.

## Admission, delegates, and exits

Enqueue requires the owner or an active scoped delegate, plus the sequencer until timed public admission has been permanently enabled. The program assigns tickets and market sequences at landing.
Admission enforces the owner nonce window and unique live intent record. Advisory ticket and market sequence fields are ignored.
Intent expiry must fall within the next 30 seconds.
Admission reserves cash or claim margin and locks accepted closes against duplicates.
Close admission requires `claim_id = 0`.

Drain processes only the exact global head and at most 16 intents.
A missing head or required record stops progress. Later tickets cannot bypass it.
Expiry, delegate revocation, missing settled positions, and pause can release intents without an oracle read.
Oracle format and guard errors defer. Missing required oracle accounts also defer.
Other CPI or account errors can reject the transaction.
Storage and arithmetic failures reject atomically. They do not create a terminal outcome.
The root retains the last 32 outcomes.
Successful admissions and terminal outcomes also emit versioned `PENCIL_EVENT_V1` logs.
Settlement logs retain position terms after the position record closes.
The finalized collector preserves these events independently of the root ring.

Delegate scopes contain expiry, market mask, leverage maximum, budget, and epoch.
Delegate opens consume margin budget at admission.
An unfilled open restores budget only to the same delegate and epoch.
An executed open keeps that budget consumed after its position closes.
Delegates cannot use the owner's custody, staking, reward, or governance authority.

The owner can request a fallback exit with a limit price.
After 15 seconds, any signer can publish that request through EnqueueExit without the sequencer.
The close enters ordinary FIFO and retains oracle and fill requirements.
Expiry or rejection requires another owner request.

## Emissions and staking

Eligible realized losses create PENCIL credits.
MintRewards clears owner credits and performs a matching SPL mint atomically.
`total_token_credits` records cumulative credit issuance. MintRewards does not reduce this counter.
Cash-loss and debt-loss counters partition cumulative issuance.

The flat rate is 100 PENCIL per eligible USDC through $500,000 of tracked LP equity.
Above that threshold, the marginal rate is `100 * (500,000 / tracked_LP)`, with exponent 1.
The program integrates rates through each event under actual LP retention, including exposure reserves and previous-peak recovery.
Drawdowns raise the rate on the same curve. The legacy tail field remains reserved and does not affect issuance.
Normal losses use the loss net of funded loss fees. Busts use full margin as the basis.
Debt busts charge no loss fee.

New funded fees require an empty payable/reservation queue and enough house cash for current debt principal plus the fee.
An odd fee atom goes to the team. Stakers receive `floor(fee / 2)`.
LP profit sharing uses global winning-exposure counters and a net cash-funded profit budget.
Only growth of distributable equity above `State.lp_highwater` can be shared.
Distributable equity is tracked LP equity minus the winning-exposure reserve.
The reserve includes unloaded positions, using loaded-record deltas at storage boundaries.
The $5,000 buffer remains fully retained. Eligible profit follows `8% + 32% * min(max(E,0)/$1M,1)^2`.
The marginal share caps at 40%, so LP retains at least 60% of eligible profit.
The program integrates marginal retention with bounded integer logarithm and exponential calculations.
A winning close can release deferred funded profit.
Ineligible losses clear the unallocated budget. Destroyed IOUs never increase it.
There is no automatic capital sweep during live trading.
After every market halts and all positions and debt settle, Harvest pays the terminal residual to active stakers.
See [ownership](../docs/05-ownership.md) for recipient timing and reserve formulas.
The reward index is scaled by 1e18.

Stake enters a pending balance. Activation requires an owner signature and completed warmup.
Warmup is 3,600 seconds on mainnet, 300 on devnet, and 8 in local fixtures.
Pending stake earns no indexed fees. Activation starts at the current index.
Top-ups reset pending warmup. Unstake removes pending tokens first.
Indexed rewards survive unstaking. ClaimStaker converts funded rewards into available USDC.
Zero-holder fees remain unassigned until distribution or Harvest with active stake.
Rounding can leave funded dust. Reward claim timing can affect discarded fractional atoms.

## Rent

A closing intent can fund its owner's new position.
A closing position, then a closing intent, can fund its owner's new claim.
The successor inherits the original rent payer and the donor's full balance.
Donors cannot fund another owner or more than one successor.

A supplied existing user record can hold a rent float beyond its own rent requirement.
If the float covers the entire remaining creation shortfall, it supplies that amount.
Otherwise the transaction's rent payer supplies the shortfall.
Float-funded records without successors return their balances to the supplied float and are deleted.
Other closed records retain a retirement marker for later reclaim.
WithdrawRent requires the owner and preserves the user record's rent requirement.

## Immutable recovery

Initialize pins deployment mode and operational keys. Economic instructions work during the upgradeable trial and after Seal.
Before Seal, the loader authority can replace code, add markets, configure markets, and lift a guardian pause.
Seal removes that authority and starts permanent recovery timers.
Legacy governance and rotation tags remain disabled. Reserved fields confer no authority.
While sealed and guarded, a guardian pause permanently stops all markets.
After explicit relayer removal, the guardian cannot pause outside local fixtures.

After 24 hours without sequenced admission or heartbeat, any signer can explicitly call `RemoveRelayer`.
This call permanently enables public owner/delegate admission and retires guardian halt power. Seal and the keeper never submit removal automatically.
Any signer can publish advancing verified oracle checkpoints. After seven days and 756,000 slots without one, a market can be permanently stopped.
Outside local mode a mark more than 30% from the market's last accepted mark is rejected, and a market admits intents only within 120 seconds of its last checkpoint.
A rejected quote ends the queue head with outcome 6 once that intent's fill window has opened. Revoking a delegate does not end an intent already queued.
Initialize rejects a vault token account that has a delegate or a close authority, and markets whose fill delays differ.
Settling a position of a stopped market takes no fee and shares no profit.
Stopped positions settle at their frozen mark without external oracle calls. Pending reservations can be refunded out of order, leaving FIFO tombstones.
Funded custody withdrawals, unstaking, claims, and rent access remain independent of these services.
Underfunded claims retain the selected FIFO economics; no new solvency guarantee is introduced.
See [the recovery specification](../docs/11-recovery.md) for account details, thresholds, direct commands, and limitations.

## Bounds and evidence

V4 has no fixed count limit for users, positions, claims, or intents.
It permits 8 markets, 32 oracle accounts per market, 32 recent outcomes, and 16 actions per batch.
The shared root, integer ranges, account lists, rent, transaction bytes, and compute remain finite.
Position multipliers cannot exceed 1,000,000,000 milli, preserving arithmetic bounds across representable prices.
SDK transactions request a 256 KiB heap and estimate compute when no explicit budget is supplied.
Mainnet trial oracle actions reserve 1.4 million compute units and use one item per keeper batch.

Existing tests provide examples of implemented behavior. They are not formal proofs or an independent audit.
The property catalog separates accounting requirements from deliberate insolvency, unsecured debt, and debt-funded issuance.

Commands that use the SDK or hosted service scripts require the full application repository.
Those scripts are outside this source package.
