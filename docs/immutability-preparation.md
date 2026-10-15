# Prepare the immutable release

The mainnet program uses the 10M USDC position cap and the active PENCIL emission and LP profit curves.
The verified public source is `e18534eae0950efeae77310213eb87f62e3a4927`.
Upgrade authority remains with the saved deployment wallet. No Seal transaction is prepared or sent.
The frontend countdown remains at 18:00 UTC on 15 October 2026.

Seal permanently removes loader authority and freezes the market configuration.
Seal retains the initial relayer and guardian. After 24 hours of inactivity, anyone can explicitly call `remove_relayer`.
Time alone never removes either role. Independent callers can continue checkpoint, FIFO, liquidation, and custody work.

## Record the permanent terms

The read-only preparation command reads program data, root state, custody accounts, mint state, and verification metadata from one finalized bank.
The configuration fingerprint includes oracle identity, market accounts, conversions, multipliers, timing, custody, operational keys, leverage, warmup, and mode flags.
Ordinary balances, prices, and execution counters do not change that fingerprint.
Any change to a permanent term requires a new acceptance record.

The current configuration SHA-256 is `fbdf00ece86133d1e4dbdf8547ebb38172eab5e6e536bfd7df4b6bd517ed4508`.
The executable SHA-256 is `92f4eb5a7e8ce6ce23e9fa8dbc2e7d894b782081269c4dc5b670a99019334e11`.
The configured global leverage ceiling is 200×. Staking warmup is 3,600 seconds.
BTC, ETH, and SOL retain a common ten-slot fill delay, a 50-slot fill window, and 30-slot oracle freshness.
The [fixed-constant reference](immutable-release.md) lists the compiled limits and recovery rules.

## Complete operational acceptance

The current read-only survey passes six checks: binary identity, authority and mode, checkpoint age, custody, published metadata, and remote source verification.
These checks establish the current deployment state. They do not replace the operating workloads in the [acceptance criteria](mainnet-acceptance.md).

| Required evidence | Current result and remaining work |
| --- | --- |
| Custody and accounting | One finalized bank reconciles. Complete reconciliation under the required mixed workload remains necessary. |
| Neutral extraction | Native and compiled regressions pass. Complete the independent strategy review for this exact release. |
| Outcomes | Both recent mainnet smoke settlements finalize. Reconcile all accepted orders from the load and recovery runs. |
| Live oracle | Genuine calls pass for all three markets. Complete supported batch, failure, and transaction-version workloads. |
| Capacity | Complete the prescribed mixed-rate runs and overload reconciliation on the intended production configuration. |
| Soak | Complete the 24-hour continuous workload and resource measurements. |
| Fault recovery | Existing crash, RPC, snapshot, and incident drills provide supporting evidence. Repeat required faults during mixed traffic. |
| Elapsed recovery | Complete the actual 24-hour role-removal and seven-day oracle-timeout drills on separate test deployments. |
| Operator runbook | Complete key recovery, fresh-wallet Phantom, direct exit, withdrawal, restore, alerts, and measured SOL reserve checks. |
| Final trial | Complete seven days with the final binary, oracle accounts, and service configuration. |

The upgraded binary became active at 16:10:17 UTC on 15 October 2026.
Seven days after that time is 16:10:17 UTC on 22 October 2026.
That is the earliest trial completion if the final configuration remains unchanged and every other requirement passes.
Recovery drills use separate deployments so the production relayer can continue service.
The acceptance template leaves every gate false until its complete supporting evidence exists.

## Prepare the final command

The environment must select the saved mainnet RPC configuration. The local executable must match the verified release.
The preparation command never loads a signer. It never sends a transaction.

```bash
node scripts/prepare-mainnet-seal.mjs --deployment=artifacts/mainnet.json
```

For a new acceptance template, use a new output path:

```bash
node scripts/prepare-mainnet-seal.mjs --deployment=artifacts/mainnet.json \
  --acceptance-template=artifacts/mainnet-seal-acceptance.draft.json
```

1. Complete every gate with evidence for the recorded binary and configuration.
2. Record the actual trial start and completion times in the acceptance record.
3. Save the completed record as `artifacts/mainnet-seal-acceptance.json`.
4. Make sure that the read-only locking command passes.

```bash
node scripts/lock.mjs --deployment=artifacts/mainnet.json \
  --acceptance=artifacts/mainnet-seal-acceptance.json --strict
```

If every gate passes and the operator authorizes irreversible sealing, run the following command:

```bash
node scripts/lock.mjs --deployment=artifacts/mainnet.json \
  --acceptance=artifacts/mainnet-seal-acceptance.json \
  --lock --confirm=PNCL5aincAvxuUWfTPCbLJiJam8Gi44hPpsJzvMAeoU \
  --keypair=<saved-upgrade-authority-keypair>
```

Mainnet checks cannot be forced. The generic recovery command directs mainnet Seal through the same acceptance gate.
The locking command uses a fresh blockhash only after acceptance passes.
No signed Seal transaction remains valid for a week of preparation.

1. After Seal finalizes, make sure that the immutable flag is true and loader authority is absent.
2. Make sure that sequenced admission, closes, custody, and keeper work still succeed.
3. Retain the final source, configuration, acceptance record, and transaction evidence.
