# Deposit, claim, and withdraw USDC

Penciltrade uses native Solana USDC. Tokens on another network and bridged assets do not count as deposits into this deployment.
The [program reference](07-programs.md) lists the USDC mint and program addresses.

## Deposit and withdraw

1. Connect the wallet that owns the funds.
2. Open the deposit action.
3. Choose an amount of native Solana USDC.
4. Review and authorize the transaction in your wallet.
5. Wait for confirmation before using the new available balance.

Your wallet needs SOL for transaction fees and any required account storage.
A deposit credits your available balance. It does not automatically open a trade, stake PENCIL, or fund the house.

To withdraw, choose an amount from available USDC and authorize the withdrawal with the owner wallet.
The program sends the USDC to that wallet's token account.
A trading delegate cannot withdraw your custody funds.

## Read the balances

| Balance | What it represents |
| --- | --- |
| Available USDC | Funded cash you can use or withdraw. |
| Reserved cash | Funds committed to an admitted order that has not reached its final result. |
| Position margin | Funds committed to an open position. |
| Unpaid claims | Amounts the house owes but has not funded for withdrawal. |
| PENCIL credits | Earned tokens you can mint. These are not USDC. |
| Unclaimed staking rewards | Earned USDC rewards you can claim into your available balance. |

You cannot withdraw reserved cash, open margin, unpaid claims, or unminted token credits as available USDC.
Closing a cash-funded position returns its surviving cash principal.
Any profit the house cannot pay becomes an unpaid claim.

## Understand cash and claim collateral

A cash-funded position starts with actual USDC margin.
A claim-funded position starts with a debt obligation, not cash that the house already holds.
Losing claim value cancels that debt. It does not add USDC to the vault.
Surviving claim principal and new profit remain claims until funded.

House equity measures shared funds after its obligations. It can be negative.
Positive trading profit does not guarantee immediate payment, and FIFO repayment does not guarantee eventual payment.

## Understand fees and rewards

The trading fee is 2% of adjusted winning gain or an eligible house gain from a loss.
It is not 2% of margin or notional.
The team and stakers each receive half of funded distributable trading fees.
Older unpaid payouts and their reservations take priority over eligible fee distributions.

Claiming staking rewards credits your available USDC. Withdrawal is a separate action.
Owner-controlled custody actions do not require a current price or the relayer's trading signature.
If the website or relayer is unavailable, an independent client can submit those actions directly to Solana.
