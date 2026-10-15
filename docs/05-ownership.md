# Earn, mint, and stake PENCIL

PENCIL is a transferable Solana token. Eligible realized trading losses earn PENCIL credits for the losing trader.
Minting converts those credits into tokens in your wallet.
The token has no premint, team token allocation, vesting schedule, or fixed total supply cap.

PENCIL does not provide automatic redemption from the USDC vault.
It has no guaranteed resale price, liquidity, or buyer.
Staking provides access to eligible USDC rewards under the rules below.

## Calculate earned credits

The emission rate uses current tracked LP, which means house equity after recorded debt obligations.
The rate stays at 100 PENCIL per eligible USDC through 500,000 USDC of tracked LP.
Above that threshold, the rate decreases along this curve:

```text
r(T) = 100                         when T <= 500,000
r(T) = 100 * (500,000 / T)          when T > 500,000
```

Here, `T` is tracked LP in USDC. Negative equity also uses the flat rate of 100.

| Tracked LP | PENCIL per eligible USDC |
| --- | ---: |
| 500,000 USDC or less | 100 |
| 1M USDC | 50 |
| 2M USDC | 25 |
| 5M USDC | 10 |

An LP drawdown increases the rate along the same curve. The drawdown itself creates no credits.
The program integrates the applicable rate across each eligible loss, including changes in retained LP during settlement.

Funded losses and destroyed claim collateral can both earn credits.
An eligible voluntary loss generally excludes a funded loss-side fee. A hard bust uses the full forfeited margin.
Debt-based emissions do not represent new USDC entering the house.

## Mint and stake

1. Open the PENCIL page with the wallet that earned the credits.
2. Use Mint to receive the earned tokens.
3. Choose an amount of PENCIL to stake.
4. Wait for the one-hour warmup.
5. Use Activate when the stake is ready.

New stake earns no rewards before activation.
Adding more pending stake restarts that pending balance's warmup. Existing active stake remains eligible.
Unstaking takes effect immediately and removes pending tokens first.
Already accrued rewards remain claimable after unstaking.

## Receive USDC rewards

Active stake receives its proportional share of eligible distributions.
Rewards come from half of funded distributable trading fees and an additional share of eligible LP profit.
Your stake percentage determines your portion of those distributions.

For LP profit, the marginal staker share follows:

```text
s(T) = 8% + 32% * min(max(T, 0) / 1,000,000, 1)^2
```

Here, `T` means LP equity after reserves for winning positions. This measure differs from the emission curve's tracked LP.

| Distributable LP | Marginal staker share |
| --- | ---: |
| 100,000 USDC | 8.32% |
| 250,000 USDC | 10% |
| 500,000 USDC | 16% |
| 750,000 USDC | 26% |
| 1M USDC or more | 40% |

The curve starts at 8% and caps at 40%. At the cap, LP retains 60% of eligible profit.
These percentages divide eligible protocol profit. They are not an annual yield or a return promised on your stake.

The house first restores its 5,000 USDC buffer and its previous retained equity peak.
Winning-position reserves, unpaid payouts, and the funded profit budget can delay or prevent a distribution.
The program distributes neither existing house capital nor destroyed debt as new cash profit.

Claim moves your accrued USDC rewards into available USDC. You can then withdraw that balance.
The page's revenue history records distributions, not a guaranteed future income stream.
