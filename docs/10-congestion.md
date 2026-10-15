# Handle congestion

The program assigns tickets at landing and settles the global FIFO against the current valid mark. FIFO means first in, first out. An oracle supplies external price data. The mark is the oracle's reference trading price. This change adds offchain resource limits and recovery. It preserves the program ABI, account layout, claim order, fill windows, and liquidation economics.

The ABI defines the program's binary interface. A fill window limits acceptable price update slots. The shared writable root still limits parallel onchain work.

## Recover submissions

1. On the relayer host, use Node 22.13 or newer.
2. For the built-in `node:sqlite` store, use Node 24.
3. Set `RELAYER_STORE_PATH` to persistent local disk.
4. Keep the database, WAL, and SHM files private. They contain fully signed transactions.
5. For an online backup, keep the database and its WAL together.

The default store path appends `.sqlite` to the journal filename. The WAL is SQLite's write-ahead transaction log. SHM holds SQLite's shared-memory coordination data.

Prepare accepts an optional `requestId` with 1–128 characters. The same owner, terms, and ID return the same preparation. Different terms return 409. Leases expire at fixed times, and later preparations do not renew them. A nonce lease reserves a number for transaction preparation. A nonce is a number that distinguishes signed requests.

A failed build releases its lease. Unsigned expiry does not consume the chain nonce. Submit makes sure that the message matches exactly and all signatures are valid. Before broadcast, it durably commits the signature and signed bytes with synchronous disk writes. It returns HTTP 202 with `prepareId`, `signature`, `status`, and `statusUrl`.

Retry with the same `prepareId` and signed bytes.

That retry creates neither a new nonce nor another economic order.

Read `GET /v1/submissions/ID` for one receipt.
For a batch of at most 32 IDs, read `GET /v1/submissions?ids=ID,ID`.

| Status | Meaning |
| --- | --- |
| `submitted`, `unknown` | Pending. |
| `admitted` | Chain admission succeeded. The ticket can remain null until transaction history is available. |
| `confirmed` | An owner action succeeded. |
| `rejected` | An observed onchain failure occurred. |
| `expired` | Preparation expired before broadcast, or finalized expiry rules establish that the broadcast did not land. |

Fill and terminal execution outcomes remain authoritative onchain. The worker batches signature status reads and rebroadcasts identical bytes while the blockhash remains valid. A blockhash identifies a recent block for transaction validity. Broadcast and ticket lookup work have bounds. Polling fairness and the last broadcast time use separate durable fields. Frequent status polling therefore cannot suppress retries.

On startup, existing stores migrate the additional broadcast timestamp. After the worker makes sure that the message and signatures are valid, broadcast skips preflight. Preflight simulates a transaction before broadcast. Program failure therefore appears onchain and can consume the normal fee. RPC or journal failure never converts a known admission into rejection.

If the blockhash expires without a readable landing result, the receipt becomes `unknown` and broadcasts stop.
While it remains `unknown`, do not prepare that economic order again. Duplicate preparation risks duplicate economic execution.

Unresolved receipts retain nonce leases and can close admission at resource limits. A transaction cannot land after its blockhash expires. Finalized block height must advance 150 beyond the last valid height. A later pass must still find no status. The receipt then becomes `expired`, releases its lease, and permits another preparation. A wallet without fee funds typically reaches this result one to two minutes after submission.

Browser clients save pending identities and owner-signed bytes locally. They recover the same order after a lost reply or reload. They display pending status without claiming admission or fill. Browser storage must succeed before submission. Session setup includes its funding transfer before signing and uses the same durable path. A pending revoke retains the session key.

Strict validation accepts supported Lighthouse assertions and relocation of unchanged compute instructions.
It preserves the payer, signer set, blockhash, compute values, ordered economic instructions, and account roles.
A known admission with unreadable ticket history remains saved until ticket recovery.
Direct session-SOL sweeps remain client RPC transactions. The server database is authoritative for receipts.
The JSONL journal provides a best-effort audit copy.

## Coordinate replicas

Co-located relayers must share `RELAYER_STORE_PATH` on local disk. SQLite transactions coordinate preparation reservations and nonce leases across restarts. Broadcast and history workers also share a database limit.

1. Do not place SQLite on NFS or use independent copies.
2. For multiple hosts, route each owner exclusively to one durable relayer.
3. Divide the fleet's resource limits between those hosts.
4. Before arbitrary routing, implement a shared transactional store.

Independent databases cannot coordinate nonces. Default receipt retention is 24 hours after a terminal update, with a 100,000-row ceiling. Ambiguous submissions remain until reconciliation. Full storage rejects new preparation. An idempotency key identifies retries of the same request. It must not identify another economic order.

Permissionless fallback exits and direct chain operations can advance the owner's nonce outside this allocator. Onchain requirements remain final.

## Bound resources and admission

| Configuration | Default | Meaning |
| --- | ---: | --- |
| `RELAYER_MAX_PREPARING` | 32 | Concurrent preparation work per process. |
| `RELAYER_MAX_HTTP` | 128 | Active HTTP handlers per process. |
| `RELAYER_MAX_PREPARED` | 512 | Shared building/prepared reservations. |
| `RELAYER_MAX_PENDING` | 128 | Shared unresolved submissions. |
| `RELAYER_RESERVED_CLOSES` | 16 | Capacity withheld from opens for closes and owner actions. |
| `RELAYER_MAX_OWNER_LEASES` | 16 | Outstanding nonce leases per owner. |
| `RELAYER_MAX_BROADCASTS` | 8 | Shared worker ceiling and local broadcast ceiling. |
| `RELAYER_MAX_QUEUE` | 128 | Queue depth plus unpublished intent reservations. |
| `RELAYER_DEADLINE_BUFFER_MS` | 500 | Extra margin beyond observed inclusion and sampling latency. |
| `PENCIL_RPC_MAX_ATTEMPTS` | 4 | Total raw attempts per logical RPC call, including guard routes. |
| `PENCIL_RPC_TIMEOUT_MS` | 2500 | Total logical RPC time budget. |
| `PENCIL_RPC_MAX_IN_FLIGHT` | 32 | Raw RPC concurrency per connection. |

Capacity rejection returns 429 or 503 with `Retry-After`. Preparation workers, maps, body sizes, connections, and HTTP timeouts have bounds. Up to four preparation workers remain reserved for closes and owner actions. At least one worker remains available for opens.

Enable `RELAYER_TRUST_PROXY=1` only behind a proxy that replaces the forwarded IP header.

Without that configuration, the socket address determines the IP bucket. Admission uses a minimal shared program snapshot and clock. It reads missing users or positions directly. Financial and risk-view failures do not block this cache. A cache stores data for later reuse.

Oracle simulations use the same funded payer as the public price view. Simulations charge no network fee. Their payer must pass the RPC fee-account check. Co-signing alone requires no SOL from the sequencer. New opens need valid observations for their market and the FIFO head's market. An aged or stalled head blocks new opens until maintenance progresses.

The controller learns capacity from terminal intents cleared during periods when the queue progresses. An intent is an authorized action awaiting execution. Waiting through one order's fill delay measures latency rather than clearing capacity. Such intervals do not reduce the estimate. A separate stalled-head guard detects a stopped queue.

Projected wait uses the queue ahead divided by capacity, minus the order's concurrent fill delay, plus one clearing step. The controller includes unpublished reservations, remaining timestamp validity, the slot window, and a conservative observed slot duration. It also includes recent p99 inclusion and observation intervals. A p99 value covers 99% of observed samples. Submit repeats the deadline assessment after wallet signing.

Closes retain reserved count capacity but must still fit their deadlines. An expired close leaves its position open. Idle periods and restarts require no warmup trade or single probe. Queue bounds, nonce leases, oracle checks, deadlines, and stalled-head checks still apply. Measured queue-wait checks require recent settlement observations. Atomic submit checks include unresolved broadcasts and admissions missing from the current snapshot. Sudden outages can still expire admitted orders. Capacity estimates do not guarantee execution.

## Schedule maintenance

The keeper submits a ready drain first. It rotates bounded liquidation, fallback-exit, harvest, and reclaim work. After a nonempty wave, it refreshes once and attempts another drain within its scheduling budget. Liquidation markets and candidate positions rotate across passes. Later passes read confirmations together. Pending transactions and concurrent plans have count bounds with reserved drain capacity.

A lost send response remains tracked through the precomputed signature. `KEEPER_BATCH_SIZE` defaults to 2. Legacy transactions allow 1–4, while `KEEPER_TX_VERSION=1` allows 1–16. `KEEPER_MAX_IN_FLIGHT` defaults to 8, and `KEEPER_CYCLE_BUDGET_MS` defaults to 2000. The cycle budget limits when work starts. A bounded RPC call already in progress can finish later.

The budget guarantees neither inclusion nor elapsed time. A complete liquidation list never runs before the next drain.

With `KEEPER_TX_VERSION=1`, maintenance uses version 1 transactions under SIMD-0385. These support up to 4,096 bytes and 64 accounts through `@solana/kit` in `sdk/v1.mjs`. Eight measured orders used about 1,650 bytes. Legacy transactions allow 1,232 bytes and fit about four orders. A v1-capable cluster needs Agave 4.2 or later. Devnet and mainnet support v1, but the pinned 2.1 local validator does not.

The default transaction version therefore remains 0. A v1 message contains its compute limit, total priority fee in lamports, heap size, and loaded-data limit. A lamport equals one billionth of a SOL. Readers using `getTransaction` or `getBlock` must pass `maxSupportedTransactionVersion: 1`.

The devnet deployment uses `KEEPER_TX_VERSION=1`, `KEEPER_BATCH_SIZE=16`, and `KEEPER_DRAIN_PARALLEL=1`. A fill carries up to 16 eligible orders and requests proportional compute.

## Reduce fill latency

An order cannot fill before its delay ends. Its mark must come from that slot or later. The keeper uses these mechanisms to approach that time:

- Separate fill and housekeeping loops prevent housekeeping sends from delaying order detection. `KEEPER_HOUSEKEEPING_INTERVAL_MS` defaults to 1000 for the second loop.
- After detecting an order, the keeper retains that snapshot and fetches the blockhash and fee sample early. It estimates the slot from the clock. It sends one slot before eligibility without another read or simulation. `KEEPER_FAST_PATH=0` disables this path.
- About half the time, the first copy arrives before an eligible mark. `KEEPER_FILL_COPIES` defaults to 3 copies, one slot apart. The first copy after the mark fills the order. The others change nothing and each cost one base fee.
- The ordinary path waits for an eligible oracle mark before sending. `KEEPER_AWAIT_MARK=0` disables that wait only outside genuine-oracle operation.
- The devnet feeder uses `FEEDER_BUSY_INTERVAL_MS`, set to 400 on devnet, while orders wait. Otherwise, it uses `FEEDER_INTERVAL_MS`. Waiting for the next push contributes to devnet fill latency.

Genuine-oracle operation always checks the source slot before sending, including the cached fast path.
The source mark must cover the head's admission slot plus its fill delay.
Expired intents can clear without an available oracle.
The mainnet configuration uses one intent, one copy, and one wave. It reserves the full compute ceiling.
The multi-copy measurements above describe earlier devnet operation.

On the test host, run `node tests/devnet-fill-slots.mjs <from> <to>` from the repository root.
Replace the placeholders with the landing-slot range for the measured transactions.

The command reports exact admission-to-fill slots from transaction landing slots. Devnet median latency fell from 16 slots to 10, against a floor of 6.

While an order waits, the keeper defers housekeeping because orders have fill deadlines. Housekeeping includes liquidation scans, exits, harvests, reclaims, recovery, and checkpoints. `KEEPER_DEFER_HOUSEKEEPING_MS` defaults to 5000 and bounds this wait during continuous flow. A value of 0 disables deferral. The keeper also schedules its next pass for the head's delay boundary.

When eligible orders exceed one transaction, the keeper sends consecutive batches within the same pass. `KEEPER_DRAIN_PARALLEL` defaults to 8, ranges from 1–16, and never exceeds `KEEPER_MAX_IN_FLIGHT`. A value of 1 disables parallel waves. Each batch depends on the preceding batch's execution.

Three rules preserve wave order:

- Each batch reaches the RPC node before the next send. Simultaneous sends arrived at the leader in arbitrary order. In one trial, twelve batches produced only sixteen fills. One RPC round trip separates ordered sends.
- The keeper sizes each batch before planning its successor. V1 allows 64 accounts, and a close needs one more account than an open. About 16 opens or 13 closes fit.
- A wave includes only orders that the known mark covers. The program stops at the first uncovered order. Later batches otherwise start beyond the actual head.

Wave batches skip simulation. Each requests 120,000 compute units per order, capped at 1,400,000. Compute units measure Solana execution work. A later pass plans the next wave behind batches still in flight. An out-of-order batch changes nothing and receives a new plan. A fill without status also receives a new plan after `KEEPER_DRAIN_RETRY_MS`.

That retry delay defaults to 3000. A value of 0 waits for blockhash expiry. With `minContextSlot`, the keeper refuses state older than a fill that it already knows landed. Otherwise, a lagging provider node can show the old queue and cause duplicate planning.

<a id="throughput-settings"></a>

## Throughput configuration

These values raise defaults sized for a few orders per second. They do not change the program.

| Configuration | Default | Devnet | What it bounds |
| --- | --- | --- | --- |
| `KEEPER_DRAIN_PARALLEL` | 8 | 16 | Batches per wave |
| `KEEPER_MAX_IN_FLIGHT` | 8 | 32 | Keeper transactions planned at once |
| `KEEPER_DRAIN_RETRY_MS` | 3000 | 1500 | Wait before planning an unseen fill again |
| `KEEPER_CYCLE_BUDGET_MS` | 2000 | 4000 | Time available for one pass to send |
| `RELAYER_MIN_CAPACITY` | 0 | 40 | Floor, in orders a second, under the measured capacity |
| `RELAYER_MAX_BROADCASTS` | 8 | 128 | Order transactions broadcast at once |
| `RELAYER_MAX_PENDING`, `RELAYER_MAX_QUEUE` | 128 | 2048 | Unlanded orders, and queue depth plus reservations |
| `RELAYER_MAX_PREPARING`, `RELAYER_MAX_PREPARED`, `RELAYER_MAX_HTTP` | 32, 512, 128 | 512, 4096, 1024 | Work in progress |
| `RELAYER_MAX_OWNER_LEASES` | 16 | 64 | One owner's unlanded orders |
| `RELAYER_OWNER_PER_SECOND`, `RELAYER_OWNER_BURST` | 1, 10 | defaults (20, 40 for a benchmark) | One owner's order rate |
| `RELAYER_STORE_SYNC` | `FULL` | `NORMAL` | `FULL` syncs disk on every commit, several times per order. `NORMAL` survives process crashes but can lose recent commits during power loss |
| `PENCIL_RPC_MAX_IN_FLIGHT`, `PENCIL_RPC_TIMEOUT_MS` | 32, 2500 | 1024, 4000 | RPC calls at once, and the time limit for one |

The admission controller requires the queue ahead to fit inside the fill window. It estimates capacity from completed keeper work. That estimate cannot exceed admitted load. Without a floor, it refused more than about 3 orders per second despite higher keeper capacity.

Set `RELAYER_MIN_CAPACITY` to a rate that the pipeline demonstrably sustains.

A floor above capacity admits orders that miss their windows and return unfilled. In one devnet workload, 16 traders offered 50 opens per second for 20 seconds. The best five seconds cleared 50 to 64 orders per second. The best ten seconds cleared about 40, with no expiry. At 32 offered orders per second, every order filled. `tests/devnet-bench.mjs` repeats the measurement.

Above about 35 orders per second, the single-thread relayer saturates and requests queue for several seconds. Ordinary maintenance uses the cycle snapshot and fits a FIFO prefix within the legacy 1,232-byte packet limit. It simulates compute and shrinks oversized or compute-heavy batches before broadcast. The compute request includes 15% margin within the chain limit.

Recent priority fees for the root account set a bounded maintenance fee. The bounds are `KEEPER_MIN_MICROLAMPORTS=1000` and `KEEPER_MAX_MICROLAMPORTS=100000`. Changing them changes the keeper's SOL cost. Fees remain fixed after signing. Following simulation, maintenance broadcasts without another preflight. A compute or state race can produce a failure at `confirmed` commitment and permit a fresh plan.

The worker does not retain an unbroadcast signature until blockhash expiry. Failed maintenance can consume normal transaction fees.

## Test the operating limits

1. From the repository root, run `npm run test:sdk`.
2. Run `npm run test:congestion`.
3. Run `npm run test:submissions`.

These deterministic suites run production JavaScript with injected chain and RPC outcomes. They neither execute SBF nor measure chain TPS. SBF is Solana's compiled program format. TPS means transactions per second.

On the real local validator host, run the following command from the repository root.
`RATES=1,2,4,8,16,32 PENCIL_BENCH_SECONDS=20 bash bench/run.sh bench/load.mjs`

RATES supplies offered rates, and PENCIL_BENCH_SECONDS supplies each phase duration. The harness schedules fixed arrivals, mixes opens and closes, and separates offered load from recovery. It reports client work limits separately from protocol rejection. Missing outcomes receive `terminal-unobserved`, never expiry or fill. Unresolved or unobserved orders fail the run. A ramp without admitted orders also fails.

Equal-price feeder pushes use distinct messages while reusing a blockhash. Each push can therefore advance its quote timestamp. The stub oracle does not replace live Phoenix account, CPI, or hosted RPC-guard testing. A cross-program invocation, or CPI, calls another Solana program. The fuzz harness retains randomized direct-chain, accounting, and oracle tests. It also supports idempotent pending receipts.

Before raising limits, measure the following below, near, and above observed capacity:

- Queue age and fill, expiry, and rejection counts.
- P99 inclusion and complete maintenance-cycle latency.
- Actual compute, RPC traffic, and memory.

1. Add RPC throttling, wallet delays, and oracle outages.
2. Add replica failures, restarts, and mass busts.
3. Reconcile every order through recovery.
4. Run frontend typecheck, tests, and build with locked dependencies.

The 32-outcome history index can lose events. Complete history requires a separate durable chain index.
