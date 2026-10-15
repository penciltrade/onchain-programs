# Mainnet service monitoring

The monitor operates on the same host as the relayer, keeper, and finalized collector.
It runs every 10 seconds under systemd supervision.
It never loads signing keys or sends blockchain transactions.
Host failure requires recovery on another host. This configuration has no second host.

## Signals and recovery

| Signal | Threshold | Response |
| --- | --- | --- |
| Worker process or local relayer failure | Three consecutive failed samples | Restart the affected worker. |
| RPC failure or stale bank | Three consecutive failed samples | Verify the alternate endpoint, preserve the stores, and restart the workers on that endpoint. |
| Repeated failure | Six consecutive failed samples | Start the incident agent. Coalesce further agent requests for 15 minutes. |
| Repeated restart | Three attempts per worker in 15 minutes | Stop automatic restarts for that worker until the budget recovers. Escalation continues. |
| Checkpoint age | 85 seconds for P1, 120 seconds for P0 | Restart the keeper when the observed oracle remains valid. Escalate invalid sources without restart loops. |
| FIFO head without progress | 20 seconds for P1, 45 seconds for P0 | Restart the keeper within its restart budget. Preserve every signed order. |
| Finalized collection delay | 120 seconds | Restart the collector within its restart budget. |
| Unresolved signed receipts | At least 120 seconds since their last update | Escalate. Never allocate a replacement economic identity. |
| Operator fee reserve | Less than three estimated days, or less than the role's minimum reserve | Alert. Funding failures never trigger service restart loops. |
| Custody or accounting mismatch | One consistent bank snapshot | Send an immediate P0 alert. Escalate repeated failures to the agent. |
| Free runtime storage | Less than 1 GiB for P1, 256 MiB for P0 | Alert and escalate. |
| Runtime snapshot age | 30 minutes | Alert and escalate. |

Restarts have a 60-second cooldown for each worker.
RPC switches also have a three-attempt budget per 15 minutes.
Independent failed workers can restart during the same pass.
An unexplained trading-readiness failure can trigger recovery even while the process remains alive.
The monitor checks the public API every 30 seconds.
A public routing failure does not restart a healthy local relayer.

The RPC check requires mainnet genesis, the correct program account owner, canonical USDC, and genuine Hawkeye.
It rejects a bank clock more than 30 seconds from the host clock.
The active route remains in private durable state.
The monitor does not switch back while the backup remains healthy.
If that route fails, a healthy primary can replace it.

The emergency route defaults to the Solana public mainnet endpoint.
Its account scans, genuine oracle simulation, and finalized history passed commissioning.
Public RPC rate limits do not establish capacity for a busy service.
Set `PENCIL_MAINNET_FAILOVER_RPC_URL` to an independent production endpoint for sustained traffic.

## Financial signals

The monitor reads the root, token vaults, Clock, and operator balances from one bank.
USDC custody must cover funded user cash, cash margin, house cash, team fees, and staker fees.
Stake custody must cover active and pending stake.
Tracked equity must equal house cash less outstanding, pending, and open debt.
Unsolicited token transfers can create uncredited excess. Negative house equity alone does not fail these checks.

Maintenance fee estimates use the quiet checkpoint budget and the highest observed hourly wallet outflow.
The simulation payer uses a fixed SOL reserve because simulations charge no network fee.
Its manual trades and refundable upgrade rent do not become recurring service costs.
Account rent remains separate from spendable SOL.
Fund the keeper for actual traffic. The quiet budget alone does not cover heavy trading.
The monitor sends an alert before estimated runway reaches three days.

## Private configuration

Keep these values in the existing private mainnet environment file:

```text
RELAYER_HEARTBEAT_INTERVAL_SECONDS=3600
RELAYER_WALLET_VALIDATION=strict
RELAYER_PUBLIC_TRADING=0
PENCIL_MAINNET_FAILOVER_RPC_URL=<mainnet endpoint>
OPS_DISCORD_WEBHOOK_URL=<Discord incoming webhook>
OPS_AGENT_COMMAND=<incident-agent-executable>
```

The heartbeat interval accepts 60–21,600 seconds. Its default is one hour.
Heartbeats run only after Seal while the sequencer remains required.
Sequenced admissions also renew the 24-hour inactivity timer.
Time alone never removes the relayer or guardian.

The mainnet service launcher installs all workers, the monitor, the incident agent, and the snapshot timer.
All managed release overrides select the same source checkout.
The private `runtime/rpc-client.env` file selects a fallback RPC for workers after failover.
Keys, submission stores, and finalized history retain their existing paths.

## Agent and Discord escalation

The incident agent receives a sanitized JSON snapshot after repeated failures.
Its shell, application, browser, and web tools are disabled.
Its remaining filesystem sandbox is read-only.
The runner excludes mainnet credentials and key paths from the child environment.
It stops the agent after five minutes and records the result privately.

The agent classifies the incident and recommends concrete next steps.
It cannot restart services, deploy code, change protocol parameters, transfer funds, or Seal the program.
The watchdog performs only the bounded recovery actions in the table.
P0 diagnoses reach Discord. The webhook also receives reserve alerts and recovery notices.
Discord mentions remain disabled.
Failed deliveries remain in a private durable queue. The monitor retries after 30 seconds.
Synthetic commissioning messages carry a `TEST` label.

## Database snapshots

The snapshot timer runs every 15 minutes and retains eight complete snapshots.
Python's SQLite backup API includes committed WAL transactions without stopping the services.
Each snapshot contains compressed receipt and outcome databases, integrity checks, hashes, and deployment identity metadata.
An incomplete snapshot cannot replace a complete snapshot.
Files remain private.

These snapshots protect local recovery from database damage or an incorrect change.
They remain on this host. They do not survive total host or disk loss.
Before production scaling, add a separate destination for encrypted snapshots.

## Public opening

If the launch checks pass, set `RELAYER_PUBLIC_TRADING=1` in private configuration.
Then restart the relayer from the reviewed release.
Public mode requires strict wallet validation.
It removes trial registration and its 50-wallet ceiling while retaining the program's trading rules.
It does not Seal the program.
Closes, withdrawals, and owner recovery retain their existing paths.

The strict validator accepts Lighthouse assertion opcodes 2–15 and relocation of unchanged compute instructions.
It preserves the payer, signer set, blockhash, compute values, ordered economic instructions, and account roles.
The recorded Phantom messages passed offline replay before rollout.

The keeper observes the genuine source slot before a paid mainnet drain, including on its cached fast path.
Expired intents can still clear when the oracle is unavailable.
This prevents the known premature-drain pattern. Landing races can still require another attempt.
