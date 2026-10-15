# Check addresses and permissions

Penciltrade runs on Solana mainnet and uses native USDC.
Use the addresses below to identify this deployment and distinguish it from unrelated tokens or programs.

| Item | Address |
| --- | --- |
| Trading program | `PNCL5aincAvxuUWfTPCbLJiJam8Gi44hPpsJzvMAeoU` |
| USDC mint | `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v` |
| PENCIL mint | `5YtqUuYrZQWBngASqB1tkATyJX5cLrGFfiSwAeUBt8nS` |
| Protocol USDC vault | `4GL6UKGZvb4U1RboZmFVH2nZqWHVHgKfC75Hc2jPU1Ba` |
| Team fee recipient | `FtVKLnTU3E3rhwyKK1iKEqvXd4yyWbNzwpZzp6EpXZqP` |

Use the website's deposit action or a compatible client to credit your account.
A direct token transfer to the vault does not create a recorded user deposit.

The [program explorer](https://solscan.io/account/PNCL5aincAvxuUWfTPCbLJiJam8Gi44hPpsJzvMAeoU) shows the deployed program and its authority.
The [public source repository](https://github.com/penciltrade/onchain-programs) contains the program source.
[Source verification](https://verify.osec.io/status/PNCL5aincAvxuUWfTPCbLJiJam8Gi44hPpsJzvMAeoU) compares a published build with the deployed executable.
That comparison is not a security audit or a guarantee against defects.
The program has no independent security audit.

## Know who can do what

Your wallet authorizes trades and controls withdrawals.
A scoped trading delegate can act only within its assigned permissions, expiry, budget, markets, and leverage.
It cannot withdraw USDC, transfer your PENCIL, or change the account owner.

The relayer co-signs normal order admission. It does not choose settlement prices or gain authority to withdraw your custody funds.
It can delay or refuse trading service. Owner fallback exits provide a separate closing path.
Independent callers can publish checkpoints, process queued orders, submit liquidations, and repay eligible claims.

While a loader upgrade authority exists, it can replace the program's code.
Seal permanently removes that authority and freezes the program and market configuration.
After Seal, a guardian can permanently halt markets while its role remains active.
The guardian cannot choose an arbitrary settlement price or withdraw user balances.

After 24 hours without included relayer admission or heartbeat, anyone can explicitly remove the relayer and guardian roles from a sealed deployment.
Time alone does not remove them. The change is permanent once that transaction succeeds.
The [outage guide](11-recovery.md) describes the resulting recovery paths.

External dependencies, including USDC and the configured oracle programs, retain their own authority policies.
Making Penciltrade immutable does not make those external programs immutable.
