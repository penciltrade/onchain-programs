# Track USDC

USDC uses six decimals. All transfers use ordinary SPL Token accounts. A program-derived address, or PDA, identifies a program-controlled account. The state PDA controls the USDC vault, PENCIL mint authority, and staking vault. The vault holds the USDC that backs client balances. PENCIL balances become actual SPL balances after reward minting.

## Separate cash and debt

Accounted USDC is cash assigned within the program's ledger. Its invariant is:

```
Accounted USDC = funded user balances
              + cash margin reserved or open
              + funded team/staker rewards
              + genuine house cash
USDC vault >= accounted USDC
```

The program tracks unpaid claims separately. A claim is an owner's unpaid settlement amount. It also separately tracks debt collateral for pending or open trades. Neither counts as USDC in the vault. House equity includes those contingent liabilities and can be negative. A contingent liability depends on a later outcome.

| Event | Cash movement | Debt movement |
| --- | --- | --- |
| Deposit | Wallet USDC enters the vault and increases the funded balance. | None. |
| Cash open | Available balance becomes reserved or open margin. | None. |
| Cash loss | Surviving principal returns. Lost cash funds the house and eligible fees. | None. |
| Cash win | Principal returns. House cash pays the profit that it can fund. | Unpaid net profit becomes a new claim. |
| Claim repayment | House cash becomes the owner's funded balance. | The oldest claim decreases. |
| Debt open | No USDC enters. | The claim becomes reserved or open contingent collateral. |
| Debt loss | No fresh USDC enters. | The loss cancels debt. Surviving debt settles at a new priority. |
| Debt win | The house pays the amount that it can fund after older claims. | Surviving principal and unpaid net profit become a new claim. |
| PENCIL mint | No USDC enters. | No automatic claim repayment occurs. |

Users cannot withdraw open margin, pending reservations, unpaid claims, or unminted token credits as USDC. A delegate cannot withdraw, change the owner, or move custody of ownership tokens. The relayer has no instruction for vault withdrawals.

## Assign cash in priority order

An IOU is an unpaid debt obligation. An IOU loss cannot create house cash. Cash losses serve existing claims in FIFO order. FIFO means first in, first out. While that queue remains, the program blocks payments to younger closes and allocations of new fees. The program does not use a second token account for this priority.

Funded available balances and funded accrued rewards remain protected from ordinary trading settlement. The ledger already assigns that cash, so it is unavailable as house capital. Before and after each mutation, the program makes sure that the actual vault covers accounted cash. Unsolicited token transfers remain uncredited excess. They neither seed the house nor disable later actions.

New fees require an empty queue for payouts and reservations. House cash must also cover all issued IOU principal plus the fee, including debt locked in open positions. This rule prevents fee payments when existing contingent liabilities exceed cash. The program cannot reserve unknown future profits that have no cap.

## Distinguish tracked LP from cash

Tracked LP measures equity for emissions and profit allocations. LP refers to liquidity in the shared house. Eligible settled loss raises tracked LP. Settled gains and funded reward allocations lower it. Only retained cash profit builds LP equity. Repayment of a recorded claim does not count that gain twice.

Debt cancellation can improve tracked equity without adding cash or triggering profit allocation. For reachable states, the review target is `tracked_lp = house_cash - (claims + pending_debt + open_debt)`. Each aggregate liability field uses `u64`. Their debt sum uses `u128`. Reservations count once through pending debt. A claim's `reserved` field supplies its FIFO barrier and does not add another debt amount.

A positive tracked number does not establish an equal amount of spendable USDC. The House tab displays house cash, net equity, queued debt, locked debt, vault coverage, and fees. It compares vault cash with accounted cash. It does not display tracked LP itself.

## Account for implementation limits

The program rejects integer operations outside their defined ranges. Settlement and ownership emissions round down to the smallest token unit. Version 2 stores users, positions, intents, claims, and proposals in separate PDAs funded through rent. Rent funds the storage that an account occupies. The root records global totals and 32 recent outcomes.

Each instruction adjusts global liabilities from changes in the records that it loads. Records absent from a transaction therefore retain their liabilities in solvency calculations. Arithmetic or account-capacity failures reject the transaction atomically. Atomic rejection leaves all transaction changes unapplied. Transaction size, account indexing, rent, and shared-root contention still limit throughput.

[Recovery](11-recovery.md) specifies permanent market wind-down, frozen-mark settlement, reservation refunds, timed public admission, and independent custody access. These termination rules belong to v3. Claims without full funding retain their existing repayment limits.
