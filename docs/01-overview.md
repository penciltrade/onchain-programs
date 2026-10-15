# PENCILTRADE

PENCILTRADE uses the trading model from Papertrade's Martingaler design. A synthetic trade creates price exposure without buying the asset. Traders choose a market and trade against shared protocol funds. These shared funds form the house. Each position limits its loss to its own margin. Margin is the value committed to a position.

The recorded devnet deployment lists BTC, ETH, and SOL. A fill is the execution of a trading order. Fills use the guarded Phoenix Perps mark after admission. The mark is the oracle's reference trading price. A slot is a scheduled Solana block opportunity. Each admitted order has a slot window for its fill price.

Liquidation closes a position at its loss threshold. Liquidations use the current guarded mark. Settlement records the resulting balances in the protocol's USDC ledger. The program does not open a corresponding Phoenix position or hedge the trade.

The build includes a native Solana program, a JavaScript SDK, a relayer, a keeper, an oracle feeder, and this interface. The relayer co-signs orders for admission. The keeper submits maintenance transactions. In production, the program reads the Phoenix Hawkeye oracle. An oracle supplies external price data.

Devnet uses a separate stub oracle with test prices. The stub returns the same data format. Devnet refuses the real Hawkeye oracle. The Oracle Lab reports mainnet observations from a separate collection.

## Trading and ownership rules

An intent is an authorized action awaiting execution. A claim is an owner's unpaid settlement amount. FIFO means first in, first out. A program-derived address, or PDA, identifies a program-controlled account.

A checkpoint records a price that the program accepts. A hard bust forfeits margin and closes the position. A haircut reduces a winning trade's gain.

| Decision | Implementation |
| --- | --- |
| House seed | Zero. User deposits remain user funds. |
| Debt | No economic issuance limit. Unpaid winnings enter owner-bound FIFO claims. |
| Leverage | 1–200× with isolated margin that cannot change. |
| Position duration | Indefinite with no funding payment or timeout. |
| Execution | Guarded Phoenix mark from a slot window after admission. Fills do not use the best bid or ask. |
| Intent order | Authenticated sequences for each market and one deterministic global ticket order. |
| Fill timing | Admission occurs first. The keeper fills orders a few slots later within a bounded slot window. |
| Liquidation | Only the current accepted checkpoint applies. A hard bust forfeits all margin. |
| Winning close | The original nonlinear haircut depends on the price move. A fee then takes 2% of the adjusted gain. |
| Losing close | The full raw loss applies up to the isolated margin. No additional trader charge applies. |
| Claim trading | Unpaid claims can fund positions. The remaining old claims retain their priority. |
| Ownership | Eligible realized losses produce PENCIL. Destroyed debt also qualifies. |
| Transferability | PENCIL is an ordinary transferable SPL token immediately after mint. |
| Team allocation | No premint exists. The team receives half of distributable trading fees. |
| Authority | The program remains upgradeable until Seal. Sealed code and configuration cannot change. Guardian halt power ends only after explicit relayer removal. |
| Recovery | After 24 hours of inactivity, anyone can explicitly remove the relayer and guardian roles. Oracle wind-down requires seven days and 756,000 slots. |

This release has no audit. V4 stores records in separate PDAs without fixed counts for users, positions, intents, or claims. Arithmetic, account size, rent, and compute remain finite. The design permits debt without an economic issuance limit and allows negative house equity. FIFO determines repayment order. It does not guarantee future cash.

## Differences from Papertrade

The design retains zero seed, unpaid profit claims, trades funded by debt, the original gain haircut, and full-margin busts. It also retains ownership emissions from losses and fee priority. Solana SPL custody and PDAs replace EVM deposit proxies. The maximum leverage falls to 200×. Ownership tokens become transferable immediately.

PENCILTRADE adds persistent intent admission. Fills use the Phoenix mark within a window anchored to admission. Papertrade uses a midpoint from a single orderbook. Papertrade's public documentation describes signed intents that a relayer and BatchExecutor dispatch. It does not establish that trading orders use this persistent AMQ. AMQ refers to the persistent queue for admitted actions.

Papertrade documents FIFO for unpaid payouts. PENCILTRADE explicitly defines both the intent queue and the claim queue.

## Read the reference chapters

| Chapter | Subject |
| --- | --- |
| Trade | Entries, closes, and busts. |
| Queues | Admission and payouts. |
| Accounting | Funded balances and debt. |
| Ownership | Token emissions and staking. |
| Oracle | Price guards and measured data. |
| Programs | Instructions and account boundaries. |
| Operations | Deployment, services, and the relayer contract. |
| Recovery | Finalization, public admission, wind-down, and direct withdrawals. |

Sources include the [Papertrade documentation](https://docs.papertrade.xyz/), the [Martingaler whitepaper](https://docs.papertrade.xyz/martingaler-whitepaper.pdf), and the [Phoenix onchain programs](https://docs.phoenix.trade/sdk/on-chain-programs).

The source defines the implemented behavior. These chapters describe v4. The repository's `formal-verification/` directory defines the review model and property catalog. Those properties await verification. Historical test reports do not prove properties for the current revision.
