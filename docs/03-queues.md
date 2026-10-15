# Understand orders and profit claims

An order passes through submission, admission, and execution.
Your signature authorizes its terms. The relayer co-signs admission and submits the transaction.
The program decides whether an admitted order can execute at an eligible oracle price.

## Read an order's status

| Status | Meaning for you |
| --- | --- |
| Prepared | The transaction is ready for your approval. No trade exists yet. |
| Submitted | The signed transaction is in progress. Its final chain result is not yet known. |
| Admitted or queued | The program accepted the order into the execution queue. It has not necessarily filled. |
| Filled | The requested opening or closing action executed. |
| Expired or rejected | The order did not execute. Inspect its final result before submitting another order. |

The website retains unresolved submissions so it can recover the same transaction after a lost reply or reload.
A slow response does not prove that your transaction failed.
Wait for a terminal result before treating funds as available for another action.

## Understand execution order

Orders use first in, first out, or FIFO. Smaller and larger orders share the same arrival priority.
Close orders also join this queue. An order cannot skip an earlier ticket because its margin is larger.

An admitted order waits ten slots before its price window starts. The window then spans 50 slots.
A slot is a Solana block opportunity. These 60 slots take about 24 seconds at 400 milliseconds per slot, but timing varies.
Orders expire within 30 seconds, and both the time deadline and price-window rules apply.

The displayed price can change before execution. Limits and oracle guards can prevent a fill.
Expired orders release their reserved funds when the program processes the final result.
An expired close leaves the position open.

## Receive an unpaid profit

An unpaid profit claim is separate from a trading order.
The house pays older claims before younger claims. A payment can repay only part of a claim.
New house funds can continue repayment later. Queue priority does not guarantee future funding or a repayment date.

You can use an eligible claim as margin for another position.
Its reserved portion cannot receive repayment while it funds that order or position.
An expired opening restores the reservation's original priority.
After a claim-funded position closes, its surviving principal and any new profit receive the close's new payout priority.

The [balance guide](04-accounting.md) explains which amounts you can withdraw as USDC.
