# Public opening checklist — 15 October 2026

The planned opening is 12:00 noon America/New_York, or 16:00 UTC.
The opening remains upgradeable. Permanent Seal requires the separate acceptance gates.

## Before public admission

| Item | Required result | Current disposition |
| --- | --- | --- |
| Integrated release | One reviewed source commit for the relayer, keeper, collector, and monitor | All four services run integrated release `74e4694`. The private rollout journal retains previous source, configuration, and stores. |
| Phantom desktop | Actual wallet assertions pass while economic changes fail validation | All 19 accepted captures pass the revised strict validator. Verify a fresh wallet flow after rollout. |
| Public onboarding | Ordinary wallets can deposit and trade without manual registration | `RELAYER_PUBLIC_TRADING=1` is prepared. It remains disabled until the opening. |
| Position cap | Program, SDK, frontend, ABI, and published source agree | The deployed program, SDK, frontend source, and ABI use the 10M USDC cap. |
| Operator funding | Fund the intended traffic and retain an operating reserve separate from rent | The keeper has about 4.7 quiet days at the latest measurement. Additional public traffic requires more SOL. |
| Health | BTC, ETH, and SOL report valid observations and fresh checkpoints | All three markets passed the latest readiness check. Continue observation through launch. |
| Custody | Vault balances and tracked equity reconcile from one bank | The latest USDC, stake, and equity checks matched exactly. |
| End-to-end trading | Deposit, long, short, close, withdrawal, and receipt recovery succeed | The 1 USDC, 50× long closed after 10.009 seconds. All actions finalized. Repeat short and fresh-wallet Phantom flows before public admission. |
| Recovery | A supervised failure recovers with the original keys and order stores | Relayer recovery passed in 24,465 ms. RPC failover passed in 24,474 ms. All 26 earlier signed order identities remained unchanged. |
| Agent and alerts | Repeated failures reach the agent and critical results reach Discord | The supervised synthetic agent exited successfully. Discord delivery passed. The durable notification queue is empty. |
| Databases | Consistent snapshots restore committed WAL records | The timer is active. A live snapshot restored 63 receipt rows and 7,928 finalized transactions with matching hashes and identities. |
| Capacity | Observe increasing mixed traffic and preserve closes under load | Sustained genuine-mainnet capacity remains unproven. Do not publish the historical 16 orders/second target as measured capacity. |
| Rollback | Retain source, service overrides, private configuration, and receipt identities | The previous release remains preserved privately. Never restore an older receipt database over live orders. |

[Commissioning evidence](../artifacts/mainnet-ops-20261015.json) records the tests, live drills, balances, and transaction signatures.
The final checks passed 180 backend tests, 72 frontend tests, two snapshot tests, and the mainnet frontend build.
The earlier operations rollout left program code unchanged. The economics upgrade below now supersedes that executable.
The existing program requires at least 1 USDC of opening margin. The relayer now rejects smaller requests before signing.

## The completed program upgrade

The upgrade finalized at slot 457299186. The complete executable matches the pinned public build byte for byte.
The public native source is `e18534eae0950efeae77310213eb87f62e3a4927`.
The release preserves funded program records and retains upgrade authority.
The relayer, keeper, collector, and monitor use the integrated release. Public admission remains disabled.

The same batch uses a flat emission rate of 100 PENCIL per eligible USDC through $500k of tracked LP.
Above $500k, the rate is `r(T) = 100 × (500,000 / T)`, where `T` is current tracked LP in USDC.
This policy replaces the earlier $120M squared tail. Existing credits and funded account layouts remain compatible.
The [upgrade scratchpad](../scratchpad.md) defines integration, boundary, and drawdown tests for that change.

The batch also changes the marginal LP profit share to `8% + (40% - 8%) × (T / $1M)^2`.
This share caps at 40%. Here, `T` remains LP equity after winning-exposure reserves.
The $5,000 buffer, funded-profit rules, previous-peak requirement, and payout priority remain in force.

The saved deployment wallet pays upgrade fees and buffer rent. All keys and durable signed identities remain saved privately.
The 380,413-byte buffer required a temporary 1.93314828 SOL deposit under the current mainnet rent schedule.
The upgrade closed that buffer and returned its deposit. The deployer retained about 1.965 SOL after the upgrade.


## Opening procedure

1. Resolve the cap upgrade disposition and complete the checks for the selected executable.
2. Fund the keeper, relayer, and oracle simulation wallet for the intended traffic.
3. Confirm current readiness, custody, snapshots, and alert delivery.
4. Keep strict validation active and enable public trading in private configuration.
5. Restart only the relayer and confirm that public configuration disables the trial gate.
6. Use a fresh wallet to complete a small deposit, open, close, and withdrawal.
7. Observe FIFO age, expired orders, liquidation work, unknown receipts, RPC failures, and SOL runway during the opening.

## Work after the opening

The [operational acceptance criteria](mainnet-acceptance.md) remain mandatory before Seal.
They include measured capacity, a 24-hour soak, at least seven days of the final configuration, independent economics review, and elapsed recovery drills.
These gates cannot finish before today's opening.
The existing upgrade authority remains until the later sealing decision.

This host runs the [operations monitor](mainnet-operations.md).
It has no second-host failover or off-host snapshot destination.
The public emergency RPC is a recovery route, not a demonstrated production backup at high load.
