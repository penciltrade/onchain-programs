# Release immutable code

The deployment starts with an upgradeable trial. After acceptance passes, Seal permanently freezes code and configuration. Seal retains the initial relayer and guardian. The frontend continues to use the relayer. After Seal, no upgrade, governance, or key-rotation path exists.

After 24 hours without included sequenced admission or heartbeat, anyone can explicitly call `remove_relayer`. That transaction permanently enables owner/delegate admission and retires guardian halt power. The guardian must satisfy the same timeout. Time alone does not change authority. The keeper reports eligibility but never submits removal. A later heartbeat cannot restore either role.

FIFO means first in, first out. The queue follows onchain tickets, and order size never changes priority. After removal, direct admission uses the SDK or recovery CLI. The current frontend still requires a relayer. Independent relayers and keepers can continue after the initial operators stop.

## Identify fixed constants

Constants reside in `program/src/types.rs`, `program/src/math.rs`, and program validation code. After Seal, a change requires a separate deployment with fresh state. Notional is margin multiplied by leverage. A checkpoint records a price that the program accepts. A basis point, or bp, equals 0.01%.

| Constant or rule | Mainnet value |
| --- | --- |
| Relayer removal eligibility | 86,400 seconds without sequenced admission or heartbeat. explicit transaction required |
| Oracle wind-down eligibility | 604,800 seconds and 756,000 slots without an advancing checkpoint |
| Admission checkpoint age | At most 120 seconds |
| Consecutive accepted mark move | At most 3,000 bp, or 30% |
| Intent lifetime | At most 30 seconds |
| Owner fallback exit delay | 15 seconds |
| Leverage | 1–200x, subject to the initialized maximum |
| Minimum margin | 1 USDC |
| Position notional | At most 10,000,000 USDC |
| Configured position multiplier | At most 1,000,000,000 milli |
| Fill delay and window bounds | 1–25 delay slots and 1–50 window slots. common delay across markets |
| Stake warmup | 3,600 seconds |
| Eligible trading fee | 2%. half to the pinned team recipient and half to stakers |
| Profit buffer | 5,000 USDC of distributable equity |
| Starter staker share | 8% curve intercept. The buffer and funding rules govern distributions |
| Profit curve | 8% + 32% × min(max(distributable LP, 0) / 1,000,000, 1)² |
| Share above the curve | 40% allocation. 60% retention |
| Loss-credit emission | 100 PENCIL through 500,000 USDC tracked LP. Above that threshold, 100 × 500,000 / tracked LP |
| Reward and exposure scales | `1e18` reward index. `1e12` normalized exposure |
| Root allocation ceiling | 65,536 bytes |
| Market and account bounds | 8 markets. 32 oracle accounts per market |
| Batch and recent outcome bounds | 16 actions. 32 recent outcomes |
| Token decimals | 6 for USDC and PENCIL |
| Oracle read cache | At most one committed Hawkeye observation per market per execution slot |

The curve integrates new eligible profit funded with cash. It distributes neither existing capital nor IOU losses. An IOU is an unpaid debt obligation. Global winning exposure reduces distributable equity. Deferred profit cannot escape through paired settlement that closes the losing leg first.

Failed transactions reverse cache writes. Their attempted oracle calls and offchain simulations remain outside the committed-read cap. A cache stores an observation for later reuse. Exact nonlinear haircut, deadband, bust arithmetic, and emission formulas reside in `program/src/math.rs`. Reserved governance-delay fields grant no operational authority.

Mainnet mode pins canonical Circle USDC, Hawkeye, and Phoenix program identities. External program upgrades and account changes remain outside Penciltrade's immutable code. After Seal, oracle accounts and market configuration cannot migrate.

## Read the initialized mainnet parameters

The deployed mainnet markets use the values below. `sdk/mainnet-profile.mjs` supplies matching defaults.
These values remain adjustable during the trial and become fixed at Seal. The deployment artifact records the explicit initialized values.

| Parameter | Initialized mainnet value |
| --- | --- |
| Common fill delay | 10 slots |
| Fill window | 50 slots |
| Mark and index freshness | 30 slots |
| Mark/index deviation | 300 bp |
| Spread field | 0. unused by the current mark-based pricing rule |
| BTC position multiplier | 814,598 milli |
| ETH position multiplier | 483,979 milli |
| SOL position multiplier | 250,000 milli |
| Global leverage ceiling | 200x |

A slot is a scheduled Solana block opportunity. The combined 60-slot window equals approximately 24 seconds at 400 ms per slot. Slot duration varies. Live testing must establish inclusion and source-age headroom. Headroom is the remaining capacity before a limit.

Mainnet preflight obtains oracle accounts and price ticks from genuine Phoenix metadata. Custody accounts, the fee recipient, operational keys, mint authority, and mode flags also become permanent.

## Complete the release

1. Deploy the deterministic build from the fresh public source repository with loader authority intact.
2. Initialize fresh v4 state and genuine mainnet accounts.
3. Run the controlled trial and all [operational acceptance gates](mainnet-acceptance.md).
4. Publish the exact source commit.
5. While loader authority remains available, complete explorer build verification.
6. Before Seal, resolve pauses, halted markets, and configuration changes.
7. Seal the deployment with the acceptance-gated lock command.
8. Make sure that loader authority removal succeeds.
9. Make sure that sequenced admission continues.

After Seal, the initial relayer can continue service. Included heartbeats preserve the guarded phase. After 24 hours of inactivity, any caller can explicitly remove both privileged roles. Community callers can publish checkpoints, drain FIFO, liquidate, harvest, and access funded custody without either role. After removal, only objective oracle timeout recovery can halt a live market.

Local test mode retains a guardian stop for settlement fixtures. Mainnet identity requirements reject that mode. The release does not depend on continuing hosted service. Immutable code can preserve specified recovery paths. It cannot repair an arbitrary future accounting or custody defect.
