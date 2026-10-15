# Cache Hawkeye observations by slot

A cache stores an observation for later reuse. A slot is a scheduled Solana block opportunity. V4 stores each market's first committed Hawkeye observation for an execution slot in the program-owned root. Drain, liquidation, and CheckpointOracle share it across instructions and transactions. Reusing it requires no Hawkeye CPI. A cross-program invocation, or CPI, calls another Solana program.

The program does not require an idle read every slot.

## Define the read cap

The cap covers committed PENCILTRADE observations on one bank history. BTC, ETH, and SOL each have a separate cache. The read slot differs from the mark and index source slots. The mark is the oracle's reference trading price. Original source slots still determine freshness, delayed fills, and oracle liveness.

The cap cannot limit independent callers, RPC simulations, abandoned forks, or failed transactions. Failed transactions reverse cache writes, so retries can repeat the CPI. [Solana transaction semantics](https://solana.com/docs/core/transactions) require this distinction.

Count committed reads separately from failed CPI attempts and offchain simulations.

## Store and reuse observations

Each Market stores its read slot, configuration version, mark, mark source slot, index source slot, and guard status. The canonical root owner and address protect these fields. Quotes supplied by wallets cannot authorize execution. The cache requires no extra account, rent funding, or client account plan.

Guard status distinguishes valid quotes, rejected quotes, and price conflicts within one source slot. Rejected observations can persist when Drain successfully commits a refund or deferral. They never replace the valid recovery mark.

An intent is an authorized action awaiting execution.

The program applies this sequence:

1. Read the execution Clock and market configuration.
2. If a committed observation exists for this slot, reuse its result.
3. If its configuration version differs, refuse execution until the next slot.
4. Otherwise, call the configured oracle.
5. Apply the existing provenance, freshness, deviation, price-move, and monotonicity guards.
6. Store the observation or rejection for this execution slot.
7. Apply each intent's source-slot window, expiry, and price limit.

The first committed observation fixes the market snapshot for the whole slot. A later upstream update within the slot cannot replace it. A configuration change cannot trigger a second read in that slot. The next slot reads the new configuration.

Previous-slot observations cannot authorize ordinary cache reuse. Unchanged source marks cannot renew `last_mark_at` or the recovery timer. Custody withdrawals, pure refunds, and frozen-market settlement remain independent of live oracle success.

## Measure the remaining gates

Native tests cover separate drains, new slots, rejected observations, independent markets, original source slots, and configuration invalidation. A compiled-validator test counts one oracle CPI across repeated checkpoints in one transaction.

1. Before sealing, measure genuine Hawkeye calls in complete fills and liquidation batches.
2. Include snapshot timing within one slot in the economic strategy matrix.
3. Measure compute, loaded bytes, inclusion latency, and fills per transaction.

Offchain simulation deduplication remains separate from the onchain cap.
