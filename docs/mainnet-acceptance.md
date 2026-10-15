# Mainnet acceptance criteria

Acceptance applies to one program binary, deployment configuration, and version of the SDK and services. The current mainnet deployment remains upgradeable during operational acceptance. Permanent sealing requires every operational gate and a separate decision after the trial. An acceptance gate is a required pass condition. The [mainnet preparation review](../mainnet-prep.md) records patches, evidence, and the trial procedure.

The initial target is 16 offered orders per second across all markets. This acceptance target requires a complete sustained-capacity measurement for the deployed release.

Increase the target only after another complete capacity run.

## Bind evidence to the release

Record these items for every test run:

- The Git commit, clean source fingerprint, program SHA-256, and applicable stub SHA-256.
- Dependency lock hashes and versions of Node, Rust, SBF tools, validator, and RPC providers.
- The program ID, state address, cluster genesis hash, loader authority, deployment phase, and chain configuration.
- The oracle program, account list, tick conversion, market version, fill delay, window, and freshness bounds.
- Keeper, relayer, database durability, admission, fee, and RPC configuration.
- Start and end times, chain slots, load seed, operator interventions, and complete result files.

An oracle supplies external price data. The keeper submits maintenance transactions. The relayer prepares transactions and co-signs admission. SBF is Solana's compiled program format. A slot is a scheduled Solana block opportunity.

The deployed v4 layout contains global exposure counters, the funded profit budget, the slot cache, and `1e18` rewards. A cache stores an observation for later reuse. Earlier v3 layouts require fresh state. The deployed economics upgrade preserves existing funded v4 accounts. Historical deployment reports remain historical evidence.

## Pass correctness gates

FIFO means first in, first out. An IOU is an unpaid debt obligation. A claim is an owner's unpaid settlement amount. A fill executes an admitted trading order.

| Gate | Pass condition |
| --- | --- |
| Custody | Accounted liabilities do not exceed the vault balance. Complete reconciliation has no unexplained difference. |
| Accounting | House equity, funded balances, margin, rewards, and FIFO claims reconcile after every workload and recovery. |
| Order identity | Each accepted signed submission retains one identity through retries, restarts, and replicas. No duplicate economic execution occurs. |
| Outcomes | Every accepted submission has a complete chain history and a terminal result. Queue progress alone cannot count as a fill. |
| Price provenance | Every economic action uses the configured oracle and the correct market conversion. Cached observations also satisfy the cache rules. |
| Funding | An IOU loss cannot create funded USDC. Loss settlement cannot export existing house capital as new cash profit. |
| Neutral extraction | The focused strategy matrix in [economic patch scope](economic-patch-scope.md) passes against the compiled program. |
| Administration | Unauthorized configuration fails. The common fill delay remains consistent across market creation and configuration. |
| Locking | The loader authority and immutable flag agree. Sealed configuration cannot change, and repeated Seal does not restore authority. |

The finalized collector must preserve signatures, slots, errors, admissions, and complete outcomes for successful transactions. Successful settlement events must retain closed position terms. Acceptance uses finalized history. Provisional interface snapshots cannot establish completeness. The collector must reject incomplete logs and unavailable history, then resume when complete evidence returns. Missing outcomes fail acceptance, including those lost from the 32-entry ring.

## Measure workload and capacity

1. Use the intended production host and database configuration, including `RELAYER_STORE_SYNC=FULL`.
2. Use the intended owner limits, IP limits, proxy behavior, and RPC routes.
3. Do not substitute the benchmark capacity floor of 400.

For each workload, run at least 30 minutes at each rate: 8, 16, 24, and 32 orders per second.

- At least 500 distinct owners, including owners with new accounts.
- Mixed BTC, ETH, and SOL opens and closes, with at least 40% closes.
- Continuous opens alongside fallback exits, liquidations, checkpoints, reward claims, and rent recovery.
- Bursts at twice the selected operating target, followed by a separate drain and reconciliation period.

A checkpoint records a price that the program accepts. The mark is the oracle's reference trading price.

1. Use fixed arrival times independent of server response time.
2. Record planned arrivals, actual requests, client limits, preparation failures, acceptance, admission, and every terminal chain outcome.
3. After the rate runs pass, run a 24-hour soak at the selected target.

A soak is a continuous test for stability. P99 is the value covering 99% of observed samples. Headroom is the remaining capacity before a limit. RSS measures the process's resident memory.

| Metric | Required pass condition under healthy oracle and RPC conditions |
| --- | --- |
| Preparation | P99 preparation takes at most 2 seconds. |
| Preparation to admission | P99 takes at most 5 seconds. This excludes deliberate human signing delay. |
| Eligible accepted opens and closes | At least 99.9% fill. The report cannot exclude service failures as oracle or client failures. |
| Admission to fill | P99 takes at most the configured fill delay plus 3 seconds. |
| Landing headroom | At p99, at least 2 seconds remain before applicable timestamp and slot deadlines. |
| Owner fallback exit | P99 takes at most 5 seconds from eligibility to terminal execution. The report states the configured eligibility delay separately. |
| Liquidation | At the operating target, p99 takes at most 2 seconds from the first accepted eligible mark to settlement. |
| Custody withdrawal | P99 confirmation takes at most 10 seconds under competing trading traffic. |
| Backlog | Queue age does not trend upward at the operating target. Burst backlog clears within 60 seconds after normal arrivals resume. |
| Resource use | Peak steady CPU use is at most 70% of provisioned capacity. RSS grows at most 10% after the soak's first hour. |
| Keeper work | Liquidation, exit, checkpoint, harvest, and rent work face no persistent starvation. |
| Receipts | No unexplained pending receipt or retained lease remains after final reconciliation. |

1. Report percentiles for each market, action type, and the complete workload.
2. Use independently reconstructed eligibility for the denominator.
3. Separate price-limit failures, deliberate cancellations, and invalid signatures from infrastructure failures.
4. Report every excluded category and its count.
5. Set the admission floor at no more than 70% of measured sustained settlement capacity for the tested workload.
6. After changes to batching, oracle, durability, hardware, or RPC routing, make sure that the floor still meets that limit.

Above capacity, admission must reject excess requests before accepting signed orders. Closes and custody actions must retain reserved capacity during overload.

## Test live oracles and transactions

1. Run full instruction batches against genuine Hawkeye and Phoenix accounts and programs on a mainnet fork.
2. Include changing account sizes, malformed returns, stale observations, source outages, and stressed prices.
3. Test batch sizes 1, 4, 8, and 16 with one market and several markets.
4. Measure full transaction compute, account count, loaded bytes, inclusion latency, and fees.
5. Require at least 20% measured compute headroom for each supported batch.
6. Reduce unsupported batch sizes.
7. Do not report stub measurements as live-oracle capacity.

The keeper budget must cover actual oracle and settlement compute, including a fresh read after a cached simulation. The initial mainnet trial forces one item per batch and reserves the full compute ceiling for oracle actions. That restriction remains until complete wrapped-call measurements support another configuration.

The v1 sender and history reader must pass on a v1-capable validator and production RPC routes.

Use Agave 4.2.2 or later for that test.

[Solana v1 guidance](https://solana.com/upgrades/larger-transaction-sizes) explains RPC and message configuration. The [slot cache design](oracle-slot-cache.md) defines a separate gate. Compiled cache tests pass for the deployed release.

During the trial, make sure that committed read counts meet the cap with genuine mainnet oracles.

Failed attempts and offchain simulations remain outside that cap.

## Test recovery and durability

During mixed trading traffic, run each fault.
While a close, withdrawal, or checkpoint remains in flight, repeat each fault.

| Fault | Required behavior |
| --- | --- |
| Relayer crash after durable acceptance | Restart recovers the same signed identity. No accepted receipt disappears. |
| Lost broadcast reply | Retry uses identical bytes and the same signature. Chain admission is not reported as rejection. |
| Keeper crash | A replacement keeper resumes work without duplicate settlement or a permanent FIFO stall. |
| RPC delay, 429, stale node, or outage | Bounded work and retries preserve receipts. New risk admission stops before resource exhaustion. |
| One unavailable RPC route | A healthy independent route can restore observation and publication. |
| No SOL for a trader or operator | The failure is visible. Expired broadcasts release leases after the finalized expiry rule permits it. |
| Blockhash expiry and fork changes | No processed-only error becomes a terminal receipt. No ambiguous order receives a new economic identity. |
| Host or storage failure | A restored database retains accepted receipts and leases. The backup includes the SQLite WAL consistently. |
| Multiple relayers and keepers | Same-host relayers share one transactional store. Duplicate workers preserve identities, limits, and settlement. |
| Quiet market | Advancing checkpoints continue without trades. Repeated source slots cannot renew the recovery timer. |
| Oracle failure during continuous traffic | Expired reservations clear. Healthy markets and custody paths remain usable under the configured queue policy. |

1. Require active service recovery within 60 seconds after a healthy replacement or RPC route becomes available.
2. Where applicable, require receipt reconciliation within 5 minutes after RPC recovery and the finalized expiry safety margin.
3. Preserve unresolved identities throughout the outage.
4. Before locking, complete elapsed-time drills for the final production configuration.
5. Test early rejection and successful relayer removal with public and guardian callers.
6. After Seal, require sequenced frontend admission.
7. After explicit removal, require public SDK admission.
8. After removal, require guardian halt rejection outside local-fixture mode.

Native clock tests must cover exact recovery boundaries. Relayer removal requires an explicit call after 24 hours without sequenced admission or heartbeat. Oracle wind-down requires seven days without an advancing checkpoint and 756,000 elapsed slots. Time alone and keeper cycles must preserve both privileged roles. Shortened fixtures cannot satisfy elapsed-time drills.

## Complete the trial before locking

Every required alert must reach the designated operator within 60 seconds of its threshold. Operator SOL reserves must cover at least seven days at measured peak daily cost. The upgradeable trial must run for at least seven days with the final oracle and service configuration.

1. Monitor queue age, fill-window failures, unknown receipts, checkpoint age, liquidation delay, RPC health, and SOL runway.
2. Before reserves reach three days of runway, alert the operator.
3. During a relayer outage, complete direct owner exit and withdrawal drills.
4. Review the key-recovery procedure.
5. Test database restoration.
6. Record every configuration change.
7. Before acceptance, resolve every reproducible neutral strategy that exports cash.

1. After the trial and all elapsed-time drills pass, lock only the reviewed binary.
2. Retain the final evidence bundle.
3. After Seal, make sure that loader authority removal succeeds.
4. Before explicit removal, make sure that Seal preserves sequenced admission and guardian halt authority.
5. Before the irreversible transaction, resolve any pause or halted trial and its cause.
6. Before removing loader authority, publish the program source.
7. Before removing loader authority, complete explorer build verification.

A paused or halted trial must fail Seal. Mainnet `scripts/lock.mjs` requires an acceptance record bound to actual program, genesis, executable, and stable configuration hashes. All eleven gate records and a completed seven-day trial are mandatory. `--force` cannot override mainnet failures. An operator must review the evidence. The record's presence alone does not prove its measurements.
