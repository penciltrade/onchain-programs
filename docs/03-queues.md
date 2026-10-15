# Use the queues

An intent is an authorized action awaiting execution. A claim is an owner's unpaid settlement amount. The intent queue orders actions before execution. The claim queue orders unpaid value after settlement. FIFO means first in, first out.

## Admit intents in order

Every accepted intent records the following information:

- The owner and authorized signer.
- The wallet nonce, market sequence, and global ticket.
- The action and margin source.
- The position or claim identifier.
- The deadline and optional price limit.

A nonce is a number that distinguishes signed requests. Admission requires the owner or active scoped delegate and the sequencer's co-signature. A scoped delegate is a key with limited authority. The owner's nonce window extends from 64 below through 1,024 above the owner's next nonce. No duplicate live owner/nonce record can exist. The program assigns the global ticket and market sequence when admission lands onchain.

Concurrent co-signed admissions cannot conflict over those assigned counters. The owner's next nonce only increases. A program-derived address, or PDA, identifies a program-controlled account. An existing intent PDA cannot initialize again before reclamation. The program does not permanently record every used nonce. A new signed message can use an unused nonce inside the window.

Solana signatures and blockhash validity separately bound exact transaction replay. A blockhash identifies a recent block for transaction validity. Each market has an independent sequence counter. The shared house settles accepted intents in one deterministic global ticket order. The house consists of shared protocol funds. This order prevents a keeper from choosing a later market because its next trade benefits the house.

The keeper submits maintenance transactions. The root account holds global counters and accounting totals. Solana serializes changes to that account, while individual records use separate PDAs. This implementation does not establish maximum Solana throughput.

The relayer assigns the next sequence and co-signs admission. It does not choose prices, hold user assets, or trade without a user or delegate signature. It coordinates bounded nonce leases and preparation reservations in durable storage. A nonce lease reserves a number for transaction preparation. The relayer broadcasts independently and reconciles pending receipts after chain confirmation. New positions still depend on sequencer availability and fairness.

Owners can request fallback closes onchain without the sequencer.

## Fill admitted orders

Enqueue makes sure that signatures and the nonce are valid. It assigns the next ticket and market sequence, then reserves margin or a claim. It can also attempt Drain in the admission transaction. Drain processes the queue in ticket order. Outside local mode, the accepted mark must come from a later slot. The mark is the oracle's reference trading price.

A slot is a scheduled Solana block opportunity. The devnet bootstrap supplies a 10-slot delay and a 50-slot window for every market. Mainnet initialization supplies its parameters. The inclusive mark window starts at `admitted_slot + fill_delay_slots`.

If a preceding ticket exists or the guarded quote is unavailable, the intent remains pending. Anyone can submit Drain, but the keeper normally submits it. Drain executes only the exact next ticket. It stops if that head's records are missing. It also stops if the quote fails before that head's fill window opens. An out-of-order Drain does nothing and cannot skip the head.

Each fill requires a mark updated inside its slot window after admission. An intent that passes its window without a fill ends with outcome 6 and returns its cash. Wallet confirmation, network propagation, leader scheduling, account contention, and guard deferrals add latency. The protocol does not promise zero price movement between the user's click and fill.

FCFS means first come, first served. Here, FCFS follows accepted onchain tickets in landing order. It does not order Phoenix's external orderbook changes or reproduce users' click order.

The window limits the mark's update slot, rather than the transaction's execution slot. A mark from inside the window can execute later while fresh. An accepted mark newer than the window produces outcome 6. A rejected quote also produces outcome 6 at any time after the window opens. Timestamp expiry is a separate terminal condition.

## Clear blocked orders

Drain examines the oldest ticket first. A rejected quote defers the head until its fill window opens. From that slot onward, a rejected quote ends the head with outcome 6 and restores its reservation. This rule prevents an unpriceable market from blocking every other market. Initialize requires the same fill delay for all markets. Following orders are not yet fillable while the head waits, so that wait delays no eligible order.

Drain remembers a quote rejection for its remaining work. Consecutive orders for the same market then cost one oracle call. A rejected quote ends every reached head for that market with an open window. The earlier rule waited for quote recovery. Rejection frequency depends on `max_age_slots` and `max_deviation_bps`, which Initialize fixes.

At initialization, set those guards with room for normal variation.

Two conditions always defer an order because any caller can cause them. One condition is missing oracle accounts in the caller's transaction. The other is an acceptable mark that differs from the mark already consumed in the same slot. Local fixtures with zero fill delay have no window and always defer.

Every admitted intent expires within 30 seconds of admission. A keeper can clear an expired head without an oracle read. This bound requires a submitted maintenance transaction and does not guarantee inclusion latency. A deadline, failed price limit, missing closed position, or other terminal precondition can end the head as rejected or expired. The program then releases its reserved resources.

Delegate revocation or replacement stops new admissions by that key. It does not withdraw an order already in the queue. Queued orders retain the same fill and termination rules. This rule prevents cancellation after the owner sees the imminent fill price. Users cannot consume reserved cash twice. Expiry of a reservation funded with debt restores the original claim's priority.

A prepared transaction has no ticket. The program assigns and returns the ticket when admission lands. Preparations therefore cannot race for a ticket, and an unsigned preparation leaves no gap. The relayer leases distinct nonces for concurrent preparations with fixed expiry and a per-owner bound. It can reuse expired unsigned leases. Signed submissions with uncertain status retain their identity and lease.

The relayer fixes the compute-unit price in each prepared message and alone adds the sequencer signature. A compute unit measures Solana execution work. Traders cannot raise their fee or attach a tip to gain priority. Within a slot, landing order follows the leader's schedule. Independent broadcasts can overlap. Offchain receipts do not promise admission order.

## Close without the sequencer

1. Sign `RequestExit(position_id, limit_price)` with the owner wallet.
2. If the market lacks a checkpoint from the last 120 seconds, submit `CheckpointOracle` first.
3. After 15 seconds, submit `EnqueueExit` with any signer.

A checkpoint records a price that the program accepts. The close receives the next global ticket and uses the owner's stored limit. EnqueueExit consumes the request and attempts ordinary FIFO Drain. Existing work retains priority. Oracle guards still apply. They include the 120-second checkpoint rule.

The close expires after 30 seconds. After expiry or rejection, the owner must request another exit. Pause does not block exit requests. Delegates cannot request exits.

## Repay claims

Claims belong to their owners and follow settlement priority. Anyone can submit Harvest. Harvest pays the oldest claim with genuine house cash. Partial repayment leaves the remaining amount at the head. Younger claims and fee recipients cannot pass it.

Increasing claim IDs determine repayment order. The `priority` field records the originating settlement ticket. Public liquidations record ticket zero and append claims at the next claim ID. Harvest does not sort by `priority`.

Funded principal from a cash position never enters this queue. It returns to available USDC even when profit remains unpaid. Only the real owner wallet can withdraw available USDC. A session key cannot withdraw it.

## Use claims as margin

You can reserve part of your own claim as debt margin. The unreserved remainder keeps its old priority. Reserved IOU collateral remains a contingent liability, although Harvest cannot immediately repay it. A contingent liability depends on a later outcome. Until the pending open executes or expires, its old priority blocks younger claims. This preserves the priority that an expired reservation must restore.

A losing trade destroys some or all of that liability without adding USDC. At close, surviving debt principal and net profit settle at the new close priority. An expired reservation that never executes restores the old claim.

This mechanism permits trading while the house has negative equity. It also permits additional debt and token issuance without fresh cash. This version does not permit claim transfers to another wallet.

## Understand repayment limits

Claims do not expire. FIFO provides a verifiable repayment order but does not guarantee future cash. With zero seed, the first winning close can immediately create debt. The design has no aggregate credit ceiling or profit cap. Equity can remain negative indefinitely if future cash losses do not cover the debt.

[Recovery](11-recovery.md) specifies permanent market wind-down, frozen-mark settlement, reservation refunds, timed public admission, and independent custody access. These termination rules belong to v3. Claims without full funding retain their existing repayment limits.
