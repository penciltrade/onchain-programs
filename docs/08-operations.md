# Operate the deployment

V4 requires fresh state and a new program ID. Initialization pins deployment mode and operational keys. Earlier devnet addresses and reports remain historical.

Do not apply this executable to funded earlier state. Earlier layouts are incompatible.

## Build and run the suites

1. Use locked npm and Rust dependencies.
2. Use the pinned Solana toolchain.
3. On the build host, use the repository's Node 24 CI version.
4. From the repository root, run the following commands.

```bash
npm ci
bash scripts/toolchain.sh --install
source scripts/toolchain.sh
cargo test --manifest-path program/Cargo.toml --locked
npm run test:sdk
npm run test:congestion
npm run test:submissions
npm run test:recovery
npm run test:mainnet
bash scripts/run-localnet.sh --test
bash scripts/run-localnet.sh --profit-test
bash scripts/run-localnet.sh --hardening-test
```

The real-validator smoke test covers sealing, owner exits, early recovery rejection, terminal settlement, custody withdrawal, and equality between built and deployed bytes. The `--hardening-test` flag runs delayed fills outside local-fixture mode with the keeper and relayer. It exercises queue heads, oracle bands, admission freshness, position limits, and guardian rules.

Native entrypoint tests inject the chain Clock. They cover exact 24-hour and seven-day boundaries, guardian retirement, elapsed slots, and independent markets. Rust tests cover fixed-point arithmetic, accounting, recovery timing, irreversible states, and reservation restoration. Older v2 governance suites do not establish v4 behavior.

For production-duration validator tests, set `PENCIL_RUN_LONG_TIMEOUTS=1` before the validator suite.
That run takes about eight days.

## Deploy, configure, and seal

The deployment starts upgradeable. Seal permanently freezes code and configuration but retains the initial relayer and guardian. After 24 hours of inactivity, an explicit `remove_relayer` ends those roles. Time alone does not remove them.

| Behavior | Upgradeable trial | Sealed and guarded | After explicit removal |
| --- | --- | --- | --- |
| Code and market configuration | Loader authority can change them. | Fixed forever. | Fixed forever. |
| Normal admission | Sequencer co-signs. | Sequencer co-signs. | Owner or delegate signs directly. |
| Guardian pause | Reversible pause on new opens. | Permanent halt of all markets. | Refused outside local fixtures. |
| Relayer inactivity | No permanent transition. | Anyone can call removal after 24 hours. | Public admission remains permanent. |
| Missing checkpoints | No permanent wind-down. | Anyone can halt after seven days and 756,000 slots. | The same objective recovery applies. |

A checkpoint records a price that the program accepts. Seal starts recovery timers. The keeper reports removal eligibility but never submits removal. FIFO means first in, first out. Removal does not change FIFO order. The [immutable release profile](immutable-release.md) lists constants and deployment parameters.

The following commands illustrate a devnet trial and finalization. The deploy command creates keys in the directory named by PENCIL_SECURE_DIR. Later commands depend on that deployment and its artifact.

1. On the deployment host, run commands from the repository root.
2. Replace the example paths with your external key directories.
3. Before the lock command, replace the confirmation placeholder with the deployed program ID.

```bash
PENCIL_SECURE_DIR=/path/to/new-keys bash scripts/deploy-devnet.sh             # deploy, upgradeable
node scripts/lock.mjs                                                         # phase, settings and readiness; changes nothing
node scripts/configure-market.mjs --market=2 --multiplier-milli=300000        # change a market's settings
node scripts/devnet-add-market.mjs --symbol=SOL --price=150 --tick-micro=10000 --multiplier-milli=300000
PENCIL_SECURE_DIR=/path/to/those-keys bash scripts/deploy-devnet.sh --upgrade # replace the program in place
node scripts/lock.mjs --lock --confirm=<program id> --keypair=/path/to/deployer.json
node scripts/verify-deployed.mjs --deployment=artifacts/devnet.json --require-immutable
```

| Command or flag | Behavior and dependencies |
| --- | --- |
| `scripts/lock.mjs` | Makes sure that deployed bytes, loader authority, live markets, and checkpoint freshness match requirements. Mainnet also requires matching operational acceptance evidence. |
| `--force` | Cannot override mainnet requirements. Sealing requires explicit program-ID confirmation and the loader authority's signature. |
| `scripts/configure-market.mjs` | Changes one market's oracle accounts, position multiplier, fill delay, window, mark age, deviation, and spread. It can lift a pause. |
| `--dry-run` | Displays the configuration change without sending it. |
| `--seal` | Locks immediately for devnet test deployments. The deployment script rejects mainnet RPC routes. Mainnet uses the acceptance-gated lock command. |

The price tick cannot change. Existing positions retain their entry multiplier. An upgrade in place retains the state account. The replacement program must read existing state and records. Layout changes require a migration, which this repository does not provide.

1. Before locking, finalize the market list, multipliers, fill delay, window, and oracle accounts.
2. Make sure that the initialized sequencer, guardian, and team keys are correct.
3. Keep keypairs outside the repository.
4. Retain old keypairs and custody records.

Those operational keys cannot change in either phase. Deployment artifacts contain public addresses and parameters. Devnet uses the stub oracle, while mainnet pins Hawkeye/Phoenix. Seal removes the actual loader authority. Neither the deployer nor guardian can reverse it. The [mainnet preparation procedure](../mainnet-prep.md) describes a controlled mainnet trial.

## Run the services

The feeder supplies devnet stub quotes. The keeper submits permissionless maintenance. The relayer prepares wallet transactions and sequences admission. Keeper work rotates bounded liquidation, exit, harvest, rent, recovery, and checkpoint waves between drains. Drain processes admitted actions in FIFO order.

All active markets need advancing accepted checkpoints. Quiet markets also need them.
The keeper attempts renewal after 60 seconds by default. `KEEPER_CHECKPOINT_INTERVAL_SECONDS` accepts 1–80 seconds.
Outside local fixtures, admission requires a checkpoint within 120 seconds.
A keeper outage therefore stops quiet-market admission after two minutes until anyone supplies a checkpoint.
Repeated observations of the same mark slot cannot renew the seven-day oracle timer.

After that deadline, the keeper still attempts a checkpoint first. It submits oracle-free wind-down on a later pass only after three program-rejected attempts beyond the deadline. No checkpoint attempt can remain in flight. RPC faults do not count as program rejections.

After Seal, the relayer sends signed heartbeats after one hour without sequenced admission or heartbeat by default.
`RELAYER_HEARTBEAT_INTERVAL_SECONDS` accepts 60–21,600 seconds.
Its sequencing key also pays for checkpoints requested when freshness approaches expiry.

Fund the sequencing key with SOL.

After 24 hours without an included admission or heartbeat, any signer can explicitly call `remove_relayer`. The guardian cannot bypass that timeout. Owner fallback exits remain enforceable after 15 seconds while the sequencer remains active.

Place `RELAYER_STORE_PATH` on persistent local disk.
Preserve its SQLite WAL with the store.

The WAL is SQLite's write-ahead transaction log. Co-located replicas share one store. Multiple hosts need owner routing with divided limits or a shared transactional store. Independent SQLite copies cannot coordinate nonces. A nonce is a number that distinguishes signed requests. Ambiguous broadcasts retain their original identity until reconciliation.

[Congestion](10-congestion.md) describes resource configuration, maintenance sizing, and benchmarks.

## Use the wallet API

Wallets sign the exact prepared message. Chain landing assigns admission its ticket. Admission is not a fill. A fill executes an admitted trading order.

| Endpoint | Behavior |
| --- | --- |
| `GET /v1/config` | Public deployment configuration. |
| `GET /v1/state?owner=PUBKEY` | Wallet view, recovery flags, market status, and recent outcomes. |
| `POST /v1/intents/prepare` | Prepare owner or delegate terms for an open or close. |
| `POST /v1/intents/submit` | Persist signed bytes and return HTTP 202 with a durable receipt. |
| `GET /v1/submissions/ID` | Read the same identity as pending, admitted, `confirmed`, or rejected. |
| `POST /v1/tx/prepare` | Prepare custody, public maintenance, or recovery actions. |
| `POST /v1/tx/submit` | Submit the exact action that the wallet signs. |
| `POST /v1/faucet` | Supply test USDC when enabled. |
| `GET /v1/trades`, `GET /v1/series` | Supply bounded historical observations. |

Preparations expire after 45 seconds. Intents allow at most 30 seconds of chain-clock validity. Fills use the fixed slot window anchored to landing.

| Outcome | Meaning |
| --- | --- |
| 0 | Opened. |
| 1 | Closed. |
| 2 | Bust. |
| 3 | Expired. |
| 4 | Rejected or cancelled. |
| 5 | Price limit. |
| 6 | Fill window passed. |

Public recovery supports `remove-relayer`, its `remove_relayer` spelling, and the legacy `enable-permissionless` alias. It also supports `checkpoint-oracle`, `wind-down-market`, `settle-wind-down`, and `cancel-wind-down-intent`. Market actions take `market`. Settlement takes `positionId`, and cancellation takes `ticket`.

Custody actions use `amount`. The `request-exit` action takes `positionId` and optional `limitPrice`. Rent actions use amounts in SOL. The relayer does not expose governance or Seal.

## Recover directly through RPC

The SDK and `scripts/recover.mjs` work directly through RPC without hosted components. Any signer can drain, harvest, checkpoint, remove an eligible relayer, and settle stopped markets. The frontend still uses the relayer. Direct owner admission uses the SDK or recovery CLI. Only the owner can withdraw cash, unstake, mint credits, and claim rewards.

[Recovery](11-recovery.md) explains frozen-price tradeoffs, guardian stops, exact commands, and insolvency limits. Independent review, live Hawkeye CPI testing, stressed-market observations, and guarded testing precede irreversible mainnet finalization. A cross-program invocation, or CPI, calls another Solana program.
