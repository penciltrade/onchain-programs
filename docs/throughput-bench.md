# Review devnet throughput tests

These notes cover the throughput work of 2026-10-12. [10-congestion.md](10-congestion.md#throughput-settings) lists the configuration. A fill executes an admitted trading order. Filled throughput rose from about 3.5 to about 32 orders per second without expiry. At 50 offered orders per second, the best five-second rate reached 45 to 64 fills per second. The best ten-second rate was about 41, and some runs missed fill windows.

A fill window limits acceptable price update slots. No program change was necessary. Configuration alone was insufficient because hardcoded limits also required keeper, relayer, submission store, and SDK changes. The keeper submits maintenance transactions. The relayer prepares transactions and co-signs admission. The single relayer process now limits throughput, while the keeper reached 64 per second when supplied that load.

## Reproduce the workload

The sealed devnet program is `DM7kHvydbK2M55VtVSFPvHYbafyzyJxr2iBQ1HkdkSSV`. It uses a 6-slot fill delay, a 50-slot window at the program maximum, and an order lifetime of at most 30 seconds. A slot is a scheduled Solana block opportunity. During these runs, slots lasted about 200 to 220 ms. The window therefore lasted roughly 10 seconds.

One 16-core host ran one relayer, one keeper, and the stub oracle feeder through one hosted RPC provider. An oracle supplies external price data. RPC round trips took about 100 ms, with p95 about 300 ms. The provider answered 400 plain requests per second without errors. A p95 value covers 95% of observed samples.

The test used 16 saved trader wallets on one market. Each order used 1 USDC margin at 2x. Wallets signed orders and submitted them through the relayer HTTP API on localhost. Keeper transactions used version 1 with up to 16 orders per fill.

On the test host, use the saved devnet keys at the path below.
From the repository root, run the benchmark and cleanup commands.

```
export PENCIL_SECURE_DIR=$HOME/secure/devnet/penciltrade-v3c
node tests/devnet-bench.mjs --lone=4 --rates=16,32,50 --seconds=10 --close-rate=32 --sol=0.12
node tests/devnet-bench-sweep.mjs --return-sol
```

The per-owner default of one order per second caps 16 traders at 16 orders per second. This benchmark used a capacity floor of 400. That is a test value.

1. For the benchmark, set `RELAYER_OWNER_PER_SECOND=20` and `RELAYER_OWNER_BURST=40`.
2. Set `RELAYER_MIN_CAPACITY` above the tested rate.
3. After the run, restore the standing devnet defaults and capacity floor of 40.

Each open position and queued order holds about 0.0016 SOL of rent from its trader's float until closure. The `--sol=0.12` flag supported 500 opens across 16 traders. A trader without enough SOL receives the system-program error "insufficient funds". The sweep closes positions left open by overload. The `--return-sol` flag returns spare trader SOL to the deployer. `artifacts/devnet-bench-report.json` holds the report.

## Interpret counts and rates

| Measure | Meaning |
| --- | --- |
| Accepted | The relayer returned a 202 receipt. |
| Filled | The count comes from changes in open positions because each fill opens or closes exactly one position. |
| Queue passed | The execute ticket advanced past an order, whether filled or expired. |
| Best 5 or 10 seconds | The largest execute-ticket advance within a window of that length, expressed as orders per second. |
| Executed per second | Observed fills divided by elapsed time from phase start to final fill. |

The chain retains only 32 outcomes. At tens of fills per second, the benchmark misses some outcomes through `outcomeUnseen`. Executed rate includes the recovery tail after arrivals stop. It therefore remains well below peak rate.

## Compare measured results

The final run used 10 seconds at each rate.

| Offered per second | Accepted | Filled | Best 5 seconds (per second) | Best 10 seconds (per second) |
| --- | --- | --- | --- | --- |
| Lone orders (4) | 4 | 4 | n/a | n/a |
| 16 | 160 | 160 | 24 | 14.3 |
| 32 | 320 | 320 | 38.8 | 26.4 |
| 50 | 500 | 342 | 44.8 | 24 |
| Closes at 32 | 826 | 817 | 44.4 | 35.7 |

Other runs offered 50 orders per second as fixes progressed.

| Seconds | Accepted | Filled | Best 5 seconds | Best 10 seconds | Note |
| --- | --- | --- | --- | --- | --- |
| 15 | 750 of 750 | 745 | 64 | not measured | None expired |
| 20 | 994 of 1000 | 961 | 51.8 | 41.2 | None expired, 33 rejected onchain |
| 20 | 890 of 1000 | 806 | 38.8 | 31.5 | Relayer refused 110 |
| 20 | 770 of 1000 | 693 | 60.8 | 38.5 | RPC calls timing out |
| 20 | 937 of 1000 | 820 | 49.6 | 39.4 | One trader out of SOL |
| 10 | 500 of 500 | 342 | 44.8 | 24 | Final run. About 150 passed the fill window |

Lone-order latency stayed about 4.3 seconds from acceptance to fill. Under load at 32 per second, median latency was 5 to 7 seconds. At 50 per second, it was 6 to 11 seconds. Before this work, the benchmark filled 2.9 to 3.6 orders per second. The relayer refused about half the load at 8 offered orders per second.

## Explain the pipeline changes

Each discovered limit concealed the next one. These changes follow discovery order.

| Limit | Change |
| --- | --- |
| Capacity estimates did not exceed admitted load. The relayer refused more than about 3 orders per second. | `RELAYER_MIN_CAPACITY` adds an operator-defined floor. |
| Hardcoded limits allowed one order per second per owner, 32 concurrent RPC calls, and 64 reconciled submissions per pass. | Limits became configurable or scaled with `RELAYER_MAX_BROADCASTS`. |
| At about 10 orders per second, record address derivation saturated one relayer thread. Each derivation took about a millisecond. | The relayer caches derived addresses. |
| The store synced disk several times per order. | Devnet used `RELAYER_STORE_SYNC=NORMAL`. |
| Each count query scanned thousands of old admitted rows. | A time bound and index limit the query. |
| JavaScript signature authentication consumed CPU. | Node's native signature authentication runs about twenty times faster. |
| Fixed 16-order batch planning ignored the 64-account limit. About 13 closes fit, so later batches began three orders beyond the head. | The keeper sizes each batch before its successor. |
| Concurrent waves landed out of order. Twelve batches produced sixteen fills, and different priority fees did not preserve order. | Each send waits until the RPC node receives its predecessor. One round trip still permits about ten batches per second. |
| The oracle mark did not cover every planned order, so partial fills misplaced later batches. | Each wave includes only orders covered by the known mark. |
| Lagging nodes returned state older than a known fill and triggered duplicate plans. | Reads include `minContextSlot`. |
| Fast-path fills carried up to 16 orders but requested compute only for those eligible at send time. Later eligibility exhausted compute. | The transaction carries no more orders than its compute request covers. |

A cache stores data for later reuse. The mark is the oracle's reference trading price. Once the keeper kept pace, the relayer saturated again at about 35 orders per second. The second CPU pass addressed these costs:

- `serialize()` repeated signature authentication in JavaScript. The sender now assembles wire bytes directly.
- Each preparation made three usually unnecessary RPC calls for token accounts, wallet balance, and blockhash. The relayer caches token accounts. Wallet balance reads depend on a low rent float. Preparations share blockhashes for half a second.
- Each call compiled SQL statements, swept storage, and counted rows. Statements now compile once. Sweeps and counts run at most every 250 ms.
- One history search for 256 signatures exceeded the RPC limit and blocked reconciliation for the whole pass. The worker now queries groups of 64 concurrently.

## Identify current limits

The relayer uses one process and one thread. It reaches full load around 35 orders per second. At 50 offered orders per second, a preparation and submission round trip takes 3 to 8 seconds. Idle latency is about 1.4 seconds. Requests queue behind the busy thread.

The fill window lasts about 10 seconds. Once combined relayer and keeper latency approaches it, orders return unfilled. This explains losses at 50 offered orders per second and complete fills at 32.

A keeper pass takes about a second. It includes a status read, a state read, an oracle read, and sends. State reads take 150 to 500 ms and grow with record count. The keeper kept pace with admitted work. No run reached a block or account compute limit. Fills used about 30,000 to 50,000 compute units per order.

Compute units measure Solana execution work.

## Plan further tests

1. Test multiple relayer processes on one host with the shared submission store.
2. Make sure that reconciliation and sequencer heartbeats work with two workers.
3. Test a lighter read for the keeper fill loop.
4. At the next sealed deployment, assess a longer fill window.
5. Test a second RPC provider for relayer traffic.

Multiple relayer processes remain untested. Reading only root state and queued records took about 140 ms, versus 150 to 500 ms for complete state. `MAX_FILL_WINDOW_SLOTS` is 50 and assumes 400 ms slots in its sizing. A second provider can separate relayer and keeper read latency.

## Respect evidence limits

- Identical runs at 50 per second filled 68% to 99% of accepted orders. Treat 32 per second as the dependable observed rate.
- Phases lasted 8 to 20 seconds. They do not establish sustained capacity over minutes.
- The test used one market and 16 owners. Shared user records reduce accounts per batch. Distinct owners require another account per open and reduce batch size.
- The stub feeder pushed every 400 ms with queued orders. No live oracle test ran.
- Wallets without fee funds previously left `unknown` receipts and permanently retained nonce leases after blockhash expiry. Operators manually marked them expired three times. The relayer now expires them after finalized expiry with no landing. [Recovery rules](10-congestion.md#recover-submissions) describe this path. Unit tests cover it, and one devnet run exercised it.
- `RELAYER_STORE_SYNC=NORMAL` survives process crashes but can lose recent commits during power loss. The default remains `FULL`.
- Unit tests cover batch sizing, planning bounded by the mark, the capacity floor, and store queries. Ordered sends and the stale-read guard only received devnet tests.

## Account for cost

| Actor | Measured cost or cadence |
| --- | --- |
| Keeper | About 0.04 SOL across roughly 10,000 orders. This includes transactions that landed without changing state. |
| Traders | 10,000 lamports per order for two signatures. Record closure returns rent. |
| Feeder | One push every 400 ms with queued orders, otherwise every 1.5 seconds. |
