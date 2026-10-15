# Recover an immutable deployment

V4 requires fresh state and a new deployment. Earlier funded state has no migration path. Initialization pins deployment mode, operational keys, custody accounts, and the fee recipient. The loader authority can configure markets during the upgradeable trial. Seal permanently freezes code and configuration. Legacy governance and rotation tags remain disabled, and their reserved fields grant no authority.

## Finalize the deployment

The upgradeable trial permits deposits, trading, staking, and permissionless maintenance. A guardian pause stops new opens. The loader authority can lift that pause. Permanent recovery instructions fail with error 105 before Seal.

Seal makes sure that the loader-v3 Program and ProgramData accounts are canonical. It removes the actual loader authority. The current loader authority signs the transaction. An existing state-PDA authority can finalize through its PDA signature. A program-derived address, or PDA, identifies a program-controlled account. If the loader authority already removed itself after initialization, any signer can record Seal.

Seal refuses a paused or halted trial. Repeated calls do not reset recovery timers. Seal preserves relayer admission and guardian halt power. Neither role ends automatically. While the sealed deployment remains guarded, a guardian pause permanently stops every market. Explicit relayer removal later retires that halt power.

Configured Hawkeye/Phoenix programs retain their own external authority policies. The [immutable release profile](immutable-release.md) lists constants and starter parameters. [Operational acceptance](mainnet-acceptance.md) defines the trial and elapsed-time evidence required before mainnet Seal.

1. Before removing loader authority, complete explorer build verification.
2. On the deployment host, run the following commands from the repository root.
3. Replace the paths with the deployment artifact, external keypair, and reviewed acceptance record.
4. Replace the confirmation placeholder with the deployed program ID.

```bash
node scripts/lock.mjs --deployment=/path/to/mainnet.json --lock --confirm=<program-id> --keypair=/secure/deployer.json --acceptance=/path/to/acceptance.json
node scripts/verify-deployed.mjs --deployment=/path/to/mainnet.json --require-immutable
```

The first command depends on that acceptance record and loader authority. The second command makes sure that the deployed program is immutable.

## Remove the relayer explicitly

The sequencer co-signs normal admission. Every included sequenced admission renews its liveness timer.
After Seal, the relayer sends signed heartbeats after one hour without admission or heartbeat by default.
`RELAYER_HEARTBEAT_INTERVAL_SECONDS` accepts 60–21,600 seconds.
A heartbeat proves key possession and transaction inclusion. It does not prove fair service to every owner.

After 24 hours without either included action, anyone can call `RemoveRelayer`. The API exposes `remove_relayer` and `remove-relayer`. The guardian must satisfy the same timeout. The legacy `enable-permissionless` alias encodes the same instruction. Removal permanently enables owner/delegate admission and retires guardian halt power outside local test mode. A later heartbeat cannot restore either role.

Time alone does not change admission. The keeper reports eligibility but never submits removal. The initial frontend still uses the relayer. After removal, direct RPC admission uses the SDK or recovery CLI. A relayer in public mode can continue service with its own key.

Nonce, authorization, delegate scope, reservations, expiry, price windows, and FIFO requirements still apply. A nonce is a number that distinguishes signed requests. FIFO means first in, first out. Tickets preserve arrival order, and order size cannot change priority. Keeper work, Drain, liquidation, Harvest, and funded custody never require sequencer authorization.

An owner can request an exit during a sequencer outage or censorship. After 15 seconds, anyone can enqueue that close with a fresh checkpoint. A checkpoint records a price that the program accepts.

1. On the recovery host, use an RPC route for the deployment.
2. From the repository root, run the required commands below.
3. Replace artifact and keypair paths with your files.
4. Before removal, make sure that the 24-hour timeout permits it.
5. Before direct admission, remove the relayer and supply a fresh checkpoint.

```bash
node scripts/recover.mjs --deployment=/path/to/mainnet.json --keypair=/secure/wallet.json --action=status
node scripts/recover.mjs --deployment=/path/to/mainnet.json --keypair=/secure/wallet.json --action=remove_relayer
node scripts/recover.mjs --deployment=/path/to/mainnet.json --keypair=/secure/wallet.json --action=checkpoint-oracle --market=0
node scripts/recover.mjs --deployment=/path/to/mainnet.json --keypair=/secure/wallet.json --action=admit --market=0 --side=long --margin=1 --leverage=10
node scripts/recover.mjs --deployment=/path/to/mainnet.json --keypair=/secure/wallet.json --action=drain --max=2
```

## Keep oracle checkpoints advancing

Anyone can call `CheckpointOracle` to publish an accepted current mark without trading. The mark is the oracle's reference trading price. Successful fills and liquidations also publish checkpoints. A fill executes an admitted trading order. Only an advancing mark slot renews the seven-day timer. Repeated observations cannot renew it.

A slot is a scheduled Solana block opportunity. Earlier slots and changed prices within an accepted source slot fail.
The keeper attempts quiet-market renewal after 60 seconds without progress by default.
`KEEPER_CHECKPOINT_INTERVAL_SECONDS` accepts 1–80 seconds.
Admission stops after 120 seconds without a checkpoint. Anyone can restore it with a valid newer observation.

The seven-day timer measures absent protocol checkpoints. It does not prove upstream failure. A keeper outage can therefore permit wind-down while Phoenix remains healthy. Before wind-down, any participant can preserve a healthy market with advancing accepted checkpoints. Deposits, admissions, heartbeats, and old quotes cannot postpone oracle recovery.

## Wind down a market

After seven days without an advancing checkpoint, any signer can call `WindDownMarket`. At least 756,000 slots must also elapse since that checkpoint. The slot condition preserves a checkpoint opportunity after a long chain halt. A never-priced market uses the Seal timestamp. It has no valid positions to settle.

After the deadline, the keeper first attempts a checkpoint. It submits wind-down on a later pass only after three program-rejected attempts and with none still in flight. RPC faults do not count as program rejections. Wind-down requires no oracle CPI, so an unavailable oracle cannot block it. A cross-program invocation, or CPI, calls another Solana program.

Wind-down permanently stops the market and freezes its last accepted mark. Other markets can continue. If every market stops, deposits also stop. While sealed and guarded, the guardian can permanently stop all markets with `Pause`. After explicit removal, Pause fails outside local test mode. A halt that precedes removal remains permanent.

The guardian cannot set settlement prices, withdraw user cash, change rules, or restart markets. `SettleWindDown` closes a stopped market's position at the frozen accepted mark. Anyone can call it without owner authorization, a current oracle, an exit request, or FIFO progress. Every position in that market uses the same frozen price. That price can differ from the current external price.

Busts, the nonlinear haircut, loss emissions, and FIFO claims retain their accounting rules. A bust forfeits margin and closes the position. Stopped-market settlement takes no trading fee and allocates no staker profit share. Final residual distribution requires all positions to settle and all claims to clear. Settlement order therefore cannot export cash ahead of outstanding winners.

`CancelWindDownIntent` refunds pending cash or restores the original claim reservation exactly once. It can cancel any queue position. A tombstone remains until FIFO retirement. A tombstone preserves a cancelled record's place in order. Pending closes release their lock. Cancellation cannot refund twice.

Once every market stops and all positions and claims settle, Harvest allocates remaining house cash to active stakers. The following examples depend on a halted market. Withdrawal also requires funded available USDC.

1. On the recovery host, run the required commands from the repository root.
2. Replace the artifact and keypair paths with your files.
3. Replace the example market, ticket, position, and amount with the intended values.

```bash
node scripts/recover.mjs --deployment=/path/to/mainnet.json --keypair=/secure/wallet.json --action=wind-down-market --market=0
node scripts/recover.mjs --deployment=/path/to/mainnet.json --keypair=/secure/wallet.json --action=cancel-wind-down-intent --ticket=123
node scripts/recover.mjs --deployment=/path/to/mainnet.json --keypair=/secure/wallet.json --action=settle-wind-down --position=456
node scripts/recover.mjs --deployment=/path/to/mainnet.json --keypair=/secure/wallet.json --action=withdraw --amount=100
```

## Access custody without hosted services

Owner-signed withdrawal, unstaking, reward claims, token minting, and rent withdrawal require neither oracle nor relayer. The direct CLI reads known root, user, and position accounts for targeted custody and terminal settlement. For targeted cancellation, add the receipt's `--owner=PUBKEY --nonce=NONCE` fields. Untargeted cancellation, Drain, and Harvest require record scans.

Available cash and principal backed by cash remain withdrawable after settlement. Unfunded profit and principal funded with debt remain unsecured FIFO claims. Wind-down creates no cash and guarantees no full repayment. It adds no treasury bailout, reserve sweep, or proportional haircut. Immutable recovery cannot repair arbitrary corruption or a defective withdrawal instruction.

## Interpret recovery tests

Native tests inject the chain Clock for exact 24-hour and seven-day boundaries, guardian retirement, and elapsed-slot conditions. Production has no accelerated timeout configuration. The compiled suite covers loader authority removal, retained sequencing, early rejection, owner exits, and finalized outcomes. Local fixtures retain a guardian stop for terminal settlement without a week-long wait. Native entrypoint tests require rejection of guardian halt after removal outside local mode.

The compiled hardening suite uses delayed fills outside local mode with checkpoint admission and mark-band guards. The production-duration run waits through both timers. It makes sure that explicit removal succeeds.

On the validator host, run `PENCIL_RUN_LONG_TIMEOUTS=1 bash scripts/run-localnet.sh --test` from the repository root.

The environment flag enables the long timers. That run takes about eight days. Fast local runs do not satisfy production elapsed-time gates. Historical reports and older governance suites do not establish this release's behavior.
