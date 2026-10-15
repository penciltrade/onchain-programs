# Use the programs

V4 uses a native Solana Rust program with SPL Token custody. A program-derived address, or PDA, identifies a program-controlled account. Record PDAs hold rent to fund their storage. Initialize sets configuration and keys. The program then operates fully while upgradeable. Seal clears the loader-v3 upgrade authority and makes code and configuration permanent.

Use a new program ID and fresh state.
Earlier funded layouts cannot migrate in place.

## Load records and accounts

The root stores configuration, counters, aggregate totals, markets, recovery state, and 32 recent outcomes. Each transaction loads only the records that it supplies. Global totals change by the difference in that subset. Missing records cannot erase liabilities from unloaded records. FIFO means first in, first out. An intent is an authorized action awaiting execution.

| Record | PDA seed | Contents |
| --- | --- | --- |
| User | `user`, owner pubkey | Cash, pending cash, credits, stake, delegate scope, nonce, and rent float. |
| Position | `position`, u64 ID | Owner, margin source, entry, leverage, multiplier, and exit request. |
| Claim | `claim`, u64 ID | Owner-bound FIFO amount and a reservation that the program can restore. |
| Intent | `intent`, SHA-256(owner pubkey + u64LE nonce) | Ticket assigned at landing and authorized terms. |
| Proposal | Reserved legacy kind | No current instruction creates or applies proposals. |

Records begin with one byte for kind and 32 bytes for the original rent payer, followed by Borsh data. The original payer can reclaim retired records. Active user records preserve nonce and delegate history. Terminal cancellation retains a kind-2 intent action until FIFO processing consumes that ticket.

The common account prefix uses this order:

| Index | Account and permissions |
| --- | --- |
| 0 | Root, writable. |
| 1 | Actor, writable signer. |
| 2 | USDC vault, writable. |
| 3 | PENCIL mint, writable. |
| 4 | Stake vault, writable. |
| 5 | SPL Token executable. |
| 6 | Actor USDC account. |
| 7 | Actor PENCIL account. |
| 8 | Sequencer. |
| 9 | Oracle program. |
| 10 | System program. |
| 11 | Rent payer signer. |

Configured readonly oracle accounts and touched writable record PDAs follow this prefix. An oracle supplies external price data. Seal adds the readonly program, writable canonical ProgramData, and loader executable at indices 12–14. AddMarket and ConfigureMarket add the readonly program and canonical ProgramData at indices 12–13. Finalization and terminal recovery neither supply nor invoke oracle data accounts.

Initialize uses a separate list for payer, state, mints, vaults, token, System, program, and ProgramData accounts. Only the canonical loader upgrade authority can select initial configuration.

## Select an instruction

A checkpoint records a price that the program accepts. A bust forfeits margin and closes the position. Notional is margin multiplied by leverage. A tombstone preserves a cancelled record's place in order.

| Tag | Instruction | Authority or condition |
| --- | --- | --- |
| 0 | Initialize | Current loader authority, fresh state, and vault token accounts without delegates or close authorities. |
| 1 | Deposit | Owner. The live deployment must accept deposits before or after Seal. |
| 2 | Withdraw | Owner with funded available cash. |
| 3–4 | SetDelegate, RevokeDelegate | Owner. |
| 5 | Enqueue | Owner or scoped delegate, plus sequencer until public mode. Landing assigns order. The market needs a checkpoint within 120 seconds. Notional cannot exceed $250,000. |
| 6 | Drain | Any signer with the exact contiguous FIFO prefix. |
| 7 | Liquidate | Any signer with a fresh accepted bust price. |
| 8 | Harvest | Any signer with the funded contiguous claim prefix. After all markets stop and settle, it pays residual house cash to stakers. |
| 9 | MintRewards | Owner with existing credit. |
| 10–11 | Stake, Unstake | Owner. |
| 12 | ClaimStaker | Owner with accrued funded rewards. |
| 13 | ClaimTeam | Pinned fee recipient with accrued funded team fees. |
| 14 | Pause | Guardian. Before Seal, the open pause is reversible. While sealed and guarded, the halt is permanent. RemoveRelayer disables Pause outside local fixtures. |
| 15–19 | Legacy configuration/upgrade tags | Always rejected. |
| 20 | ActivateStake | Owner after warmup. |
| 21 | RequestExit | Position owner. |
| 22 | EnqueueExit | Any signer after the owner's request matures for 15 seconds. The market needs a checkpoint within 120 seconds. |
| 23 | LiquidateBatch | Any signer with a bounded list from one market. |
| 24 | ReclaimRecord | Original rent payer of a retired record. |
| 25–27 | Legacy rotation/delay tags | Always rejected. |
| 28 | WithdrawRent | Owner. The withdrawal must preserve record rent. |
| 29 | AddMarket | Loader upgrade authority before Seal. |
| 30 | Seal | Current loader authority, or any signer after external finalization. |
| 31 | RemoveRelayer | Any signer after 24 hours without sequenced admission or heartbeat. The call explicitly retires sequencing and guardian halt powers. |
| 32 | CheckpointOracle | Any signer with an advancing accepted observation. |
| 33 | WindDownMarket | Any signer after seven days and 756,000 slots without an advancing checkpoint. |
| 34 | SettleWindDown | Any signer with a halted market and its frozen accepted mark. |
| 35 | CancelWindDownIntent | Any signer with a halted market. The call refunds once and retains a tombstone. |
| 36 | SequencerHeartbeat | Pinned sequencing key. The call cannot close public mode. |
| 37 | ConfigureMarket | Loader upgrade authority before Seal. The call replaces one market's oracle accounts and risk configuration. It can lift a pause. The price tick stays fixed. |

Economic tags work before and after Seal. Recovery tags 31 and 33–35 require sealing and fail with 105 before Seal. Tags 29 and 37 fail with 104 after Seal. Disabled governance tags fail before account loading. [Recovery](11-recovery.md) explains transitions and direct commands.

## Respect bounds and interpret errors

The program supports eight markets, 32 oracle accounts per market, 16 actions per batch, and 32 recent outcomes. Records have no fixed count cap. Root contention, integer ranges, rent, compute, and account lists remain finite. Around each mutation, the program makes sure that custody accounts, funded totals, PDA identity, and token authority are valid.

| Error | Meaning |
| --- | --- |
| 100 | Invalid oracle observation or no recent checkpoint for admission. |
| 101 | Stale nonce or nonce outside its range. |
| 102 | Paused or halted trading. |
| 104 | Disabled governance or upgrade instruction. |
| 105 | Recovery unavailable before sealing. |
| 106 | Recovery time or elapsed-slot condition unmet. |

Normal Solana errors cover signatures, accounts, insufficient funds, arithmetic, and storage. Escape paths retain accounting requirements. They cannot repair arbitrary corruption or guarantee repayment of unsecured debt.
