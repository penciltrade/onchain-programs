# Manage PENCIL

PENCIL is PENCILTRADE's transferable SPL ownership token. Supply starts at zero. No premint, team allocation, VC allocation, airdrop, or vesting schedule exists. Eligible realized losses create token credits for the losing trader. MintRewards converts those credits into SPL tokens. A program-derived address, or PDA, identifies a program-controlled account.

The program PDA holds mint authority. PENCIL becomes transferable immediately after minting. Transferability does not guarantee market liquidity. A community can create an AMM pool. An AMM pool lets traders exchange assets through shared liquidity. This release provides no funded pool, token price, or guaranteed buyer.

Token ownership provides access to the defined staking revenue mechanism. It does not provide automatic redemption from the USDC vault.

## Determine eligible loss

The house consists of shared protocol funds. A claim is an owner's unpaid settlement amount. LP refers to liquidity in the house. A hard bust forfeits margin and closes the position.

| Loss type | Emission basis |
| --- | --- |
| Funded voluntary close with an empty payout queue | Raw loss minus any funded LP-side fee. |
| Funded voluntary close with an active payout queue | Full raw loss. The program waives the loss fee. |
| Funded hard bust | Full lost margin, even when the house pays a fee on the loss. |
| Voluntary close funded with debt | Destroyed claim value minus any funded fee on the loss. |
| Hard bust funded with debt | Full destroyed claim margin. No liquidation fee applies. |

Emissions from debt follow the selected original mechanics. An equal new USDC deposit does not back them. With zero seed, the program can issue claims and ownership without a corresponding cash inflow. Token resale value and farming incentives need empirical study. Farming creates trades to collect rewards.

Cumulative counters distinguish credits from cash losses and debt losses. These counters do not change the gross-loss emission policy. Paired positions can earn loss credits even when the owner's combined trades profit. The hardening suite explicitly records this behavior.

## Calculate emissions

Tracked LP measures equity for emissions and profit allocations. Below $2 million of tracked LP, the rate is 100 PENCIL per $1 of eligible loss. This flat rate also applies to negative equity. Above that threshold, the marginal rate is:

```
rate(H) = 100 * (120,000,000 / (120,000,000 + H))^2
```

`H` records cumulative progress through the tail region. The tail region covers emissions above the threshold. Progress never decreases when LP equity falls or the program distributes cash. The program splits a loss event that crosses $2 million. It integrates the rate across the tail portion instead of applying one endpoint's rate to the whole event. This reduces incentives to split events.

The program apportions differences between the emission basis and tracked gain across that split. Below $2 million of tracked LP, the flat-region rule applies again. Prior tail progress remains for future events in the tail region.

LP profit sharing slows equity growth. A crossing event uses the integrated allocation curve to calculate the profit required to reach actual $2M equity. Tail progress counts eligible LP gain after trading fees and before profit sharing. Splitting a loss or allocating profit to stakers does not reset prior progress. These rules do not establish exact parity with the original deployed code.

## Allocate trading fees

Stakers deposit PENCIL to earn eligible USDC rewards.

The team recipient and stakers each receive half of funded distributable trading fees. The underlying fee is 2% of adjusted gain or an eligible house gain from loss. It is not a fee on trade notional. Notional is margin multiplied by leverage.

Fees cannot pass unpaid claims or pending claim reservations. They also cannot pass issued debt collateral that exceeds remaining house cash. A settlement in a stopped market takes no fee. The staker share rounds down to `floor(fee / 2)`. The team receives the remainder and any odd atom. An atom is the smallest token unit.

No separate frontend or builder allocation exists. The pinned team recipient receives the operating half of the fee. The [published Papertrade simulator](https://github.com/HakaiRhinohl/PaperTrade-Simulations) labels that recipient as the frontend. Matching headline percentages does not prove parity with deployed contracts.

## Allocate LP profit

Funded LP profit contributes to staker rewards after the program reserves outstanding winning exposure. Winning exposure covers possible payouts to open winners. `E` is realized LP equity minus that reserve, in USDC. Any equity deficit and the $5,000 buffer refill before rewards receive profit.

| Distributable equity E | Marginal staker share | Marginal LP retention |
| --- | ---: | ---: |
| Below $5,000 | 0% | 100% |
| $5,000 through $100,000 | 8% | 92% |
| $550,000 | 53.5% | 46.5% |
| $1M and above | 99% | 1% |

Between $100,000 and $1M, the share rises linearly:

```
share(E) = 0.08 + 0.91 * (E - 100,000) / 900,000
```

The program integrates `1 / (1 - share(E))` to calculate retained equity for each event. Integer rounding favors LP retention. Event splitting changes allocation only through small rounding differences in USDC atoms. The SDK uses the same integer rules.

Above $1M of distributable equity, an eligible $100 cash loss pays $1 to the team and $1 in staker fees. The remaining $98 allocates $97.02 to stakers and retains $0.98. This example assumes that equity recovery and outstanding exposure do not delay allocation. Retention remains 1% above $1M, so $1M is no hard maximum on house cash. House cash can exceed distributable equity because it covers debt and winning exposure.

The high-water mark records the highest retained distributable equity. A new position cannot lower it. Profit distribution requires growth above it. Recovery from a previous equity decrease stays in the house.

Global market counters include positions funded with cash or debt. They also include records absent from the transaction.

Shorts reserve 90% of their notional. Longs reserve 90% of their entry-normalized notional at 130% of the last accepted mark. The mark is the oracle's reference trading price. These bounds include the possible winning fee and exceed current winning liability. The long bound also covers the largest next accepted mark increase.

A missing long mark blocks profit sharing. The reserve can delay rewards when leverage is high. Only eligible cash losses add to the funded profit budget awaiting allocation. Winning payouts and their fees consume that budget. A closing winner can release profit held back when the losing leg closed. The 99% rate therefore applies to funded net profit after the reserve, rather than gross paired-position losses.

A waiting payout queue, no active stake, or an ineligible loss blocks allocation and clears the unallocated budget. An IOU is an unpaid debt obligation. Destroyed IOUs do not add funded profit. Open IOU positions remain in the reserve for winning exposure. Rewards belong to holders active when the deferred allocation occurs. Each holder's saved reward index preserves fees already assigned to that holder.

While markets remain live, the program does not automatically distribute existing capital. Neither the buffer nor the reserve guarantees solvency through arbitrary future price changes. Directional profit remains uncapped. Market conditions can change liquidity between $100k and $1M.

Settlements in a stopped market allocate no profit, regardless of the mark. At the end of the program's life, the remaining house cash goes to stakers. Every market must stop, and every position must settle. Every claim or reserved claim must clear. Once these conditions hold, no remaining obligation can draw on that cash. Any signer's `Harvest` then distributes all residual cash to the stakers active at that time.

If nobody has active stake, cash remains in the house. New stake still waits through its warmup. Holders therefore have that interval to stake before the first new holder can collect.

## Stake and claim rewards

Stake moves actual PENCIL into a pending balance in the staking vault. The warmup delays eligibility for rewards. Mainnet warmup is 3,600 seconds, and devnet warmup is 300 seconds. After warmup, the owner calls ActivateStake. Activation starts the new tokens at the current reward index. Pending tokens earn no fees.

An atomic stake/liquidate/unstake cannot collect that liquidation's distribution. A top-up resets warmup for the pending balance. Existing active stake remains eligible. Unstake takes effect immediately and removes pending tokens first. Accrued rewards from active stake remain claimable after unstaking.

With no stakers, the funded staker share remains an unassigned reserve. The next distribution with active stakers assigns it to those stakers. A public Harvest after holders stake can also assign it. No prior holder loses assigned fees because none existed. This rule does not transfer another holder's indexed fees to new stake.

A reward claim moves funded reward value into the owner's available USDC balance. A withdrawal then transfers it to the owner's wallet. The $PENCIL tab presents these steps:

1. Mint your earned credits.
2. Stake your PENCIL.
3. After warmup, use Activate.
4. Claim your staking rewards.

The tab separately displays wallet PENCIL, unminted credits, staked PENCIL, the holder's share of all stake, and USDC rewards. Its chart displays minted supply and total stake.

`total_token_credits` records cumulative issuance. It does not record current unminted credits or current supply. MintRewards clears only the owner's unminted credits. SPL holders can burn tokens, so current supply can differ from cumulative minted amounts. Index and claim rounding can leave funded dust. Each reward settlement discards its fractional remainder.

[Recovery](11-recovery.md) specifies permanent market wind-down, frozen-mark settlement, reservation refunds, timed public admission, and independent custody access. These termination rules belong to v4. Claims without full funding retain their existing repayment limits.
