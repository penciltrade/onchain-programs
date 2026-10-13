use crate::{math, types::*};
use solana_program::{program_error::ProgramError, pubkey::Pubkey};
pub fn add(a: u64, b: u64) -> Result<u64, ProgramError> {
    a.checked_add(b).ok_or(ProgramError::ArithmeticOverflow)
}
pub fn sub(a: u64, b: u64) -> Result<u64, ProgramError> {
    a.checked_sub(b).ok_or(ProgramError::InsufficientFunds)
}
pub fn user_index(s: &State, owner: &Pubkey) -> Result<usize, ProgramError> {
    s.users
        .iter()
        .position(|u| &u.owner == owner)
        .ok_or(ProgramError::InvalidArgument)
}
pub fn ensure_user(s: &mut State, owner: Pubkey) -> Result<usize, ProgramError> {
    if let Some(i) = s.users.iter().position(|u| u.owner == owner) {
        return Ok(i);
    }
    s.users.push(User {
        owner,
        available: 0,
        pending_cash: 0,
        token_credit: 0,
        stake: 0,
        pending_stake: 0,
        stake_ready_at: 0,
        reward_index: s.reward_index,
        reward_credit: 0,
        nonce: 0,
        delegate: Pubkey::default(),
        delegate_expiry: 0,
        delegate_budget: 0,
        delegate_max_leverage: 0,
        delegate_market_mask: 0,
        delegate_epoch: 0,
    });
    Ok(s.users.len() - 1)
}
pub fn settle_rewards(s: &mut State, i: usize) -> Result<(), ProgramError> {
    let u = &mut s.users[i];
    let delta = s
        .reward_index
        .checked_sub(u.reward_index)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    let earned = (u.stake as u128)
        .checked_mul(delta)
        .ok_or(ProgramError::ArithmeticOverflow)?
        / REWARD_SCALE;
    u.reward_credit = add(u.reward_credit, math::u64_checked(earned)?)?;
    u.reward_index = s.reward_index;
    Ok(())
}
pub fn accrue_fees(s: &mut State, total: u64) -> Result<(), ProgramError> {
    if total == 0 {
        return Ok(());
    }
    s.house_cash = sub(s.house_cash, total)?;
    let staker = total / 2;
    s.fee_team = add(s.fee_team, total - staker)?;
    distribute(s, staker)
}
pub fn distribute(s: &mut State, amount: u64) -> Result<(), ProgramError> {
    s.fee_staker = add(s.fee_staker, amount)?;
    if s.total_stake > 0 {
        // No-holder fees have never been attributed. The first subsequent
        // distribution gives that reserve to then-active holders; assigned
        // historical fees remain owned through each user's saved index.
        let distributable = add(amount, s.reward_remainder)?;
        s.reward_remainder = 0;
        let increment = (distributable as u128)
            .checked_mul(REWARD_SCALE)
            .ok_or(ProgramError::ArithmeticOverflow)?
            / s.total_stake as u128;
        s.reward_index = s
            .reward_index
            .checked_add(increment)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        // Attribution is fixed NOW; staking later cannot acquire old fees.
    } else {
        s.reward_remainder = add(s.reward_remainder, amount)?;
    }
    Ok(())
}
pub fn loaded_totals(s: &State) -> Result<Totals, ProgramError> {
    let sum = |values: Vec<u64>| values.into_iter().try_fold(0u64, add);
    Ok(Totals {
        user_cash: s
            .users
            .iter()
            .try_fold(0u64, |v, u| add(v, add(u.available, u.pending_cash)?))?,
        cash_margin: sum(s
            .positions
            .iter()
            .filter(|p| !p.debt_funded)
            .map(|p| p.margin)
            .collect())?,
        claims: sum(s.claims.iter().map(|c| c.amount).collect())?,
        pending_debt: sum(s
            .intents
            .iter()
            .filter(|i| i.kind == 0 && i.claim_id != 0)
            .map(|i| i.margin)
            .collect())?,
        open_debt: sum(s
            .positions
            .iter()
            .filter(|p| p.debt_funded)
            .map(|p| p.margin)
            .collect())?,
    })
}
pub fn current_totals(s: &State) -> Result<Totals, ProgramError> {
    let n = loaded_totals(s)?;
    let b = s.loaded_totals;
    let t = s.totals;
    let delta = |total, before, after| add(sub(total, before)?, after);
    Ok(Totals {
        user_cash: delta(t.user_cash, b.user_cash, n.user_cash)?,
        cash_margin: delta(t.cash_margin, b.cash_margin, n.cash_margin)?,
        claims: delta(t.claims, b.claims, n.claims)?,
        pending_debt: delta(t.pending_debt, b.pending_debt, n.pending_debt)?,
        open_debt: delta(t.open_debt, b.open_debt, n.open_debt)?,
    })
}
/// Aggregate the loaded positions. Storage saves only their delta into each
/// market's global exposure, so omitting a winner cannot reduce its reserve.
pub fn loaded_exposures(s: &State) -> Result<Vec<Exposure>, ProgramError> {
    // Ledger-only fixtures without market records still cover loaded exposure.
    let count = s.markets.len().max(
        s.positions
            .iter()
            .map(|p| p.market as usize + 1)
            .max()
            .unwrap_or(0),
    );
    let mut result = vec![Exposure::default(); count];
    for p in &s.positions {
        if p.entry == 0 || !matches!(p.side, -1 | 1) {
            return Err(ProgramError::InvalidAccountData);
        }
        let notional = p.margin as u128 * p.leverage as u128;
        let e = &mut result[p.market as usize];
        if p.side == 1 {
            let numerator = notional
                .checked_mul(math::EXPOSURE_SCALE)
                .ok_or(ProgramError::ArithmeticOverflow)?;
            let units = numerator / p.entry as u128 + (numerator % p.entry as u128 != 0) as u128;
            e.long_units = e
                .long_units
                .checked_add(units)
                .ok_or(ProgramError::ArithmeticOverflow)?;
        } else {
            e.short_notional = e
                .short_notional
                .checked_add(notional)
                .ok_or(ProgramError::ArithmeticOverflow)?;
        }
    }
    Ok(result)
}
pub fn current_exposures(s: &State) -> Result<Vec<Exposure>, ProgramError> {
    let loaded = loaded_exposures(s)?;
    let mut result = Vec::with_capacity(loaded.len());
    let delta = |global: u128, old: u128, new: u128| {
        global
            .checked_sub(old)
            .and_then(|v| v.checked_add(new))
            .ok_or(ProgramError::InvalidAccountData)
    };
    for (i, e) in loaded.iter().enumerate() {
        let prior = s.loaded_exposures.get(i).copied().unwrap_or_default();
        let global = s
            .markets
            .get(i)
            .map(|m| Exposure {
                long_units: m.long_units,
                short_notional: m.short_notional,
            })
            .unwrap_or_default();
        result.push(Exposure {
            long_units: delta(global.long_units, prior.long_units, e.long_units)?,
            short_notional: delta(
                global.short_notional,
                prior.short_notional,
                e.short_notional,
            )?,
        });
    }
    Ok(result)
}
pub fn profit_reserve(s: &State) -> Result<u64, ProgramError> {
    let mut total = 0u64;
    for (i, e) in current_exposures(s)?.iter().enumerate() {
        let mark = s.markets.get(i).map_or(0, |m| m.last_mark);
        total = total.saturating_add(math::winning_reserve(e.long_units, e.short_notional, mark)?);
    }
    Ok(total)
}
pub fn distributable_equity(s: &State) -> Result<i128, ProgramError> {
    Ok(s.tracked_lp - profit_reserve(s)? as i128)
}
pub fn debt(s: &State) -> Result<u128, ProgramError> {
    let t = current_totals(s)?;
    Ok(t.claims as u128 + t.pending_debt as u128 + t.open_debt as u128)
}
pub fn queue_active(s: &State) -> Result<bool, ProgramError> {
    let t = current_totals(s)?;
    Ok(t.claims > 0 || t.pending_debt > 0)
}
pub fn can_accrue_fees(s: &State, fee: u64) -> Result<bool, ProgramError> {
    Ok(!queue_active(s)? && s.house_cash as u128 >= debt(s)? + fee as u128)
}
pub fn append_claim(
    s: &mut State,
    owner: Pubkey,
    amount: u64,
    priority: u64,
) -> Result<(), ProgramError> {
    if amount == 0 {
        return Ok(());
    }
    let id = s.next_claim;
    if !queue_active(s)? {
        s.claim_head = id;
    }
    s.next_claim = add(id, 1)?;
    s.claims.push(Claim {
        id,
        owner,
        amount,
        priority,
        reserved: 0,
    });
    Ok(())
}
pub fn pay_or_queue(
    s: &mut State,
    owner: Pubkey,
    amount: u64,
    priority: u64,
) -> Result<(), ProgramError> {
    if amount == 0 {
        return Ok(());
    }
    if !queue_active(s)? {
        let paid = s.house_cash.min(amount);
        s.house_cash -= paid;
        let u = user_index(s, &owner)?;
        s.users[u].available = add(s.users[u].available, paid)?;
        append_claim(s, owner, amount - paid, priority)
    } else {
        append_claim(s, owner, amount, priority)
    }
}
pub fn harvest(s: &mut State, max: u16) -> Result<u16, ProgramError> {
    let mut n = 0;
    while n < max && s.claim_head < s.next_claim {
        // A caller supplies a contiguous prefix. Missing records stop the batch,
        // never permit skipping an older claim or a restorable reservation.
        let Some(ci) = s.claims.iter().position(|c| c.id == s.claim_head) else {
            break;
        };
        let c = s.claims[ci].clone();
        if c.amount > 0 {
            if s.house_cash == 0 {
                break;
            }
            let Ok(u) = user_index(s, &c.owner) else {
                break;
            };
            let paid = c.amount.min(s.house_cash);
            s.users[u].available = add(s.users[u].available, paid)?;
            s.house_cash -= paid;
            s.claims[ci].amount -= paid;
        }
        n += 1;
        if s.claims[ci].amount == 0 && c.reserved == 0 {
            s.claims.remove(ci);
            s.claim_head = add(s.claim_head, 1)?;
        } else {
            break;
        }
    }
    Ok(n)
}
/// Equity level from which a loss settled now is shared with stakers: its
/// opening equity, or the LP high-water mark when that is higher. Equity that
/// is only being regained after earlier payouts is retained in full.
pub fn lp_share_base(s: &State, before: i128) -> i128 {
    before.max(s.lp_highwater as i128)
}
/// Share the part of this settlement's gain that lifts LP equity above `base`.
/// `tracked_lp` already includes the gain. The base is never negative, so the
/// equity left behind still covers every outstanding IOU with funded cash.
pub fn share_lp_profit(s: &mut State, base: i128) -> Result<u64, ProgramError> {
    let grown = distributable_equity(s)?.saturating_sub(base);
    if grown <= 0 {
        return Ok(0);
    }
    let recognized = (grown as u128).min(s.lp_pending_profit as u128) as u64;
    let share = math::lp_profit_share(base, recognized)?;
    // Retained recognized profit also becomes part of the new high-water mark.
    s.lp_pending_profit = sub(s.lp_pending_profit, recognized)?;
    s.house_cash = sub(s.house_cash, share)?;
    s.tracked_lp = s
        .tracked_lp
        .checked_sub(share as i128)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    distribute(s, share)?;
    Ok(share)
}
/// Record a new LP equity peak. Growth retained while sharing was not eligible
/// (a waiting payout queue, no active stake, a destroyed IOU) also raises the
/// mark, so it is never shared out later.
pub fn raise_lp_highwater(s: &mut State) -> Result<(), ProgramError> {
    let equity = distributable_equity(s)?;
    if equity > s.lp_highwater as i128 {
        s.lp_highwater = math::u64_checked(equity as u128)?;
    }
    Ok(())
}
/// Once every market is stopped, every position settled and no IOU remains,
/// nothing can ever draw on house cash again. Pay it to active stakers instead
/// of leaving it locked for good. With no active stake it stays where it is.
pub fn sweep_terminal_residual(s: &mut State) -> Result<u64, ProgramError> {
    let t = current_totals(s)?;
    if s.house_cash == 0
        || s.total_stake == 0
        || s.markets.is_empty()
        || !s.markets.iter().all(|m| m.halted)
        || t.cash_margin != 0
        || t.open_debt != 0
        || t.claims != 0
        || t.pending_debt != 0
    {
        return Ok(0);
    }
    let amount = s.house_cash;
    s.house_cash = 0;
    s.lp_pending_profit = 0;
    s.tracked_lp = s
        .tracked_lp
        .checked_sub(amount as i128)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    distribute(s, amount)?;
    Ok(amount)
}
pub fn mint_credit(
    s: &mut State,
    owner: Pubkey,
    basis: u64,
    lp_gain: u64,
    before: i128,
    share_from: Option<i128>,
) -> Result<u64, ProgramError> {
    let amount = if let Some(base) = share_from {
        math::emission_with_lp_profit_share(basis, lp_gain, before, base, &mut s.tail_highwater)?
    } else {
        math::emission(basis, lp_gain, before, &mut s.tail_highwater)?
    };
    let u = user_index(s, &owner)?;
    s.users[u].token_credit = add(s.users[u].token_credit, amount)?;
    s.total_token_credits = add(s.total_token_credits, amount)?;
    Ok(amount)
}
pub fn settle_position(
    s: &mut State,
    pos_index: usize,
    price: u64,
    priority: u64,
    forced: bool,
) -> Result<Outcome, ProgramError> {
    let p = s.positions[pos_index].clone();
    let user = user_index(s, &p.owner)?;
    let bust = forced; // Caller validates the current or permanently frozen mark.
                       // A stopped market settles at one frozen mark, in whatever order callers
                       // choose, and after a full stop no later loss can repay a winner. Nothing
                       // may leave the house there before every position is settled: no fee and
                       // no profit share. What is left at the very end goes to stakers anyway
                       // (sweep_terminal_residual).
    let stopped = s.markets.get(p.market as usize).map_or(false, |m| m.halted);
    let (win, value, win_fee) = if bust {
        (false, p.margin, 0)
    } else {
        math::pnl(
            p.margin,
            p.leverage,
            p.entry,
            price,
            p.side,
            p.position_multiplier_milli,
        )?
    };
    let before = s.tracked_lp;
    let share_base = lp_share_base(s, distributable_equity(s)?);
    let mut minted = 0;
    // Remove locked margin liability before requeueing any surviving debt principal.
    s.positions.remove(pos_index);
    if win {
        if p.debt_funded {
            pay_or_queue(s, p.owner, add(p.margin, value)?, priority)?;
        } else {
            s.users[user].available = add(s.users[user].available, p.margin)?;
            pay_or_queue(s, p.owner, value, priority)?;
        }
        s.tracked_lp = s
            .tracked_lp
            .checked_sub(value as i128)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        let mut charged = 0;
        if !stopped && can_accrue_fees(s, win_fee)? {
            accrue_fees(s, win_fee)?;
            s.tracked_lp -= win_fee as i128;
            charged = win_fee;
        }
        s.lp_pending_profit = s.lp_pending_profit.saturating_sub(add(value, charged)?);
        if !stopped && !queue_active(s)? && s.total_stake > 0 {
            // Closing the winner can release profit held back on its losing leg.
            share_lp_profit(s, share_base)?;
        } else {
            s.lp_pending_profit = 0;
        }
        raise_lp_highwater(s)?;
    } else {
        if p.debt_funded {
            pay_or_queue(s, p.owner, p.margin - value, priority)?;
        } else {
            s.users[user].available = add(s.users[user].available, p.margin - value)?;
            s.house_cash = add(s.house_cash, value)?;
        }
        // Existing FIFO debt is serviced before fee extraction.
        let had_queue = queue_active(s)?;
        harvest(s, MAX_BATCH as u16)?;
        let fee = if !stopped
            && !had_queue
            && !(p.debt_funded && bust)
            && can_accrue_fees(s, value / 50)?
        {
            value / 50
        } else {
            0
        };
        accrue_fees(s, fee)?;
        let gain = value - fee;
        s.tracked_lp = s
            .tracked_lp
            .checked_add(gain as i128)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        // Destroyed IOUs create equity, not funded profit to export. Funded
        // profit is shared under the fee rule: no winner may be waiting in the
        // payout queue. Open debt-funded margin does not block it, because LP
        // equity already nets every IOU and the share comes only from growth
        // above the high-water mark. With no active stake nothing is exported.
        let base = share_base;
        let profit_sharing = !stopped && !p.debt_funded && !queue_active(s)? && s.total_stake > 0;
        let mut shared = 0;
        if profit_sharing {
            s.lp_pending_profit = add(s.lp_pending_profit, gain)?;
            shared = share_lp_profit(s, base)?;
        } else {
            // Ineligible growth cannot become a later funded distribution.
            s.lp_pending_profit = 0;
        }
        minted = mint_credit(
            s,
            p.owner,
            if bust { value } else { gain },
            gain,
            before,
            (profit_sharing && shared > 0 && profit_reserve(s)? == 0).then_some(base.max(before)),
        )?;
        raise_lp_highwater(s)?;
        if p.debt_funded {
            s.debt_loss_credits = add(s.debt_loss_credits, minted)?;
        } else {
            s.cash_loss_credits = add(s.cash_loss_credits, minted)?;
        }
    }
    let outcome = Outcome {
        ticket: priority,
        code: if bust { 2 } else { 1 },
        position_id: p.id,
        price,
        pnl: if win {
            i64::try_from(value).map_err(|_| ProgramError::ArithmeticOverflow)?
        } else {
            -i64::try_from(value).map_err(|_| ProgramError::ArithmeticOverflow)?
        },
        minted_credit: minted,
    };
    crate::emit_chain_event(&ChainEvent::Outcome {
        outcome: outcome.clone(),
        position: Some(p),
    });
    Ok(outcome)
}
pub fn expected_vault(s: &State) -> Result<u64, ProgramError> {
    let t = current_totals(s)?;
    add(
        add(
            add(add(t.user_cash, t.cash_margin)?, s.house_cash)?,
            s.fee_team,
        )?,
        s.fee_staker,
    )
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn state() -> State {
        State {
            version: 4,
            bump: 0,
            local_mode: true,
            devnet: false,
            usdc_mint: Pubkey::new_unique(),
            pencil_mint: Pubkey::new_unique(),
            usdc_vault: Pubkey::new_unique(),
            stake_vault: Pubkey::new_unique(),
            sequencer: Pubkey::new_unique(),
            oracle_program: Pubkey::new_unique(),
            guardian: Pubkey::new_unique(),
            team: Pubkey::new_unique(),
            admins: [
                Pubkey::new_unique(),
                Pubkey::new_unique(),
                Pubkey::new_unique(),
            ],
            paused: false,
            immutable: true,
            permissionless: false,
            started_at: 0,
            last_sequenced_at: 0,
            max_leverage: 200,
            governance_delay: 5,
            next_ticket: 1,
            execute_ticket: 1,
            next_position: 1,
            next_claim: 1,
            next_proposal: 1,
            config_version: 1,
            house_cash: 0,
            fee_team: 0,
            fee_staker: 0,
            total_stake: 0,
            total_pending_stake: 0,
            stake_warmup: STAKE_WARMUP,
            reward_index: 0,
            reward_remainder: 0,
            tracked_lp: 0,
            tail_highwater: 0,
            total_token_credits: 0,
            claim_head: 1,
            cash_loss_credits: 0,
            debt_loss_credits: 0,
            lp_highwater: 0,
            lp_pending_profit: 0,
            totals: Totals::default(),
            loaded_totals: Totals::default(),
            loaded_exposures: vec![],
            markets: vec![],
            users: vec![],
            positions: vec![],
            claims: vec![],
            intents: vec![],
            outcomes: vec![],
            proposals: vec![],
        }
    }
    fn pos(id: u64, owner: Pubkey, margin: u64, debt_funded: bool) -> Position {
        Position {
            id,
            owner,
            market: 0,
            side: 1,
            margin,
            leverage: 100,
            entry: 100_000_000,
            debt_funded,
            opened_at: 0,
            config_version: 1,
            position_multiplier_milli: 814598,
            close_pending: false,
            exit_ready_at: 0,
            exit_limit_price: 0,
        }
    }
    fn economic(s: &State) {
        assert_eq!(
            s.tracked_lp,
            s.house_cash as i128 - debt(s).unwrap() as i128
        );
    }
    fn staked(s: &mut State, stake: u64) -> usize {
        let i = ensure_user(s, Pubkey::new_unique()).unwrap();
        s.users[i].stake = stake;
        s.total_stake += stake;
        i
    }
    #[test]
    fn funded_losses_share_only_profit_above_bootstrap() {
        let mut s = state();
        staked(&mut s, math::USDC);
        let a = Pubkey::new_unique();
        ensure_user(&mut s, a).unwrap();
        s.positions.push(pos(1, a, 5_000 * math::USDC, false));
        let vault = expected_vault(&s).unwrap();
        settle_position(&mut s, 0, 99_000_000, 1, true).unwrap();
        // Below the $5,000 bootstrap buffer only the 2% fee leaves the house.
        assert_eq!(s.house_cash, 4_900 * math::USDC);
        assert_eq!(s.fee_staker, 50 * math::USDC);
        assert_eq!(s.fee_team, 50 * math::USDC);
        assert_eq!(s.lp_highwater, 4_900 * math::USDC);
        economic(&s);
        assert_eq!(expected_vault(&s).unwrap(), vault);
        s.positions.push(pos(2, a, 1_000 * math::USDC, false));
        let vault = expected_vault(&s).unwrap();
        settle_position(&mut s, 0, 99_000_000, 2, true).unwrap();
        // $100 of the $980 gain refills the buffer; the other $880 is shared
        // at the starting rate of 8%.
        let shared = s.fee_staker - 60 * math::USDC;
        assert_eq!(shared, 70_400_000);
        assert_eq!(s.house_cash, 5_880 * math::USDC - shared);
        assert_eq!(s.lp_highwater, s.house_cash);
        assert_eq!(s.fee_team, 60 * math::USDC);
        economic(&s);
        assert_eq!(expected_vault(&s).unwrap(), vault);
    }
    #[test]
    fn ninety_nine_percent_share_is_funded_and_attributed_to_active_stakers() {
        let mut s = state();
        let a = Pubkey::new_unique();
        let i = ensure_user(&mut s, a).unwrap();
        s.users[i].stake = math::USDC;
        s.total_stake = math::USDC;
        s.house_cash = 1_000_000 * math::USDC;
        s.tracked_lp = s.house_cash as i128;
        s.lp_highwater = s.house_cash;
        s.positions.push(pos(1, a, 100 * math::USDC, false));
        let vault = expected_vault(&s).unwrap();
        settle_position(&mut s, 0, 99_000_000, 1, true).unwrap();
        // $1 of fee each, then 99% of the remaining $98.
        assert_eq!(s.fee_team, math::USDC);
        assert_eq!(s.fee_staker, math::USDC + 97_020_000);
        settle_rewards(&mut s, i).unwrap();
        assert_eq!(s.users[i].reward_credit, s.fee_staker);
        assert_eq!(expected_vault(&s).unwrap(), vault);
        economic(&s);
    }
    #[test]
    fn waiting_iou_blocks_sharing_and_open_debt_exposure_is_reserved() {
        for waiting in [false, true] {
            let mut s = state();
            staked(&mut s, math::USDC);
            let a = Pubkey::new_unique();
            ensure_user(&mut s, a).unwrap();
            s.house_cash = 1_000_000 * math::USDC;
            if waiting {
                // A winner is waiting in the payout queue (record not loaded).
                s.totals.claims = 100 * math::USDC;
                s.next_claim = 2;
            } else {
                // $1 of IOU margin in an open position: not a waiting payout.
                s.markets.push(Market {
                    last_mark: 100_000_000,
                    ..Market::default()
                });
                s.positions.push(pos(1, a, math::USDC, true));
            }
            s.tracked_lp = s.house_cash as i128 - debt(&s).unwrap() as i128;
            s.lp_highwater = s.tracked_lp as u64;
            s.positions.push(pos(2, a, 100 * math::USDC, false));
            let idx = s.positions.len() - 1;
            let vault = expected_vault(&s).unwrap();
            settle_position(&mut s, idx, 99_000_000, 2, true).unwrap();
            if waiting {
                // Same rule as fees: nothing leaves while a payout is queued.
                assert_eq!(s.fee_staker, 0);
                assert_eq!(s.fee_team, 0);
            } else {
                assert_eq!(s.fee_team, math::USDC);
                let shared = s.fee_staker - math::USDC;
                assert_eq!(
                    shared, 0,
                    "the $117 winning reserve exceeds this $98 funded gain"
                );
                assert_eq!(s.lp_pending_profit, 98 * math::USDC);
            }
            assert!(s.lp_highwater as i128 >= distributable_equity(&s).unwrap());
            assert_eq!(expected_vault(&s).unwrap(), vault);
            economic(&s);
        }
    }
    #[test]
    fn debt_destruction_does_not_pay_lp_profit_from_old_cash() {
        let mut s = state();
        let a = Pubkey::new_unique();
        ensure_user(&mut s, a).unwrap();
        s.house_cash = 10_000_000 * math::USDC;
        s.positions.push(pos(1, a, 100 * math::USDC, true));
        s.tracked_lp = s.house_cash as i128 - debt(&s).unwrap() as i128;
        let vault = expected_vault(&s).unwrap();
        settle_position(&mut s, 0, 99_000_000, 1, true).unwrap();
        assert_eq!(s.house_cash, 10_000_000 * math::USDC);
        assert_eq!(s.fee_staker, 0);
        assert_eq!(s.fee_team, 0);
        assert!(s.users[0].token_credit > 0);
        assert_eq!(expected_vault(&s).unwrap(), vault);
        economic(&s);
    }
    #[test]
    fn subset_updates_preserve_unloaded_cash_and_debt() {
        let mut s = state();
        let owner = Pubkey::new_unique();
        let u = ensure_user(&mut s, owner).unwrap();
        s.users[u].available = 20;
        s.totals.user_cash = 100;
        s.totals.claims = 50;
        s.loaded_totals = loaded_totals(&s).unwrap();
        s.users[u].available -= 5;
        let totals = current_totals(&s).unwrap();
        assert_eq!(totals.user_cash, 95);
        assert_eq!(totals.claims, 50);
        assert_eq!(expected_vault(&s).unwrap(), 95);
    }
    #[test]
    fn omitted_claims_cannot_allow_fee_extraction_or_skip_fifo() {
        let mut s = state();
        s.house_cash = 100;
        s.totals.claims = 50;
        s.next_claim = 2;
        assert!(!can_accrue_fees(&s, 1).unwrap());
        assert_eq!(harvest(&mut s, 16).unwrap(), 0);
        assert_eq!(s.house_cash, 100);
        assert_eq!(s.claim_head, 1);
    }
    #[test]
    fn zero_seed_winner_principal_is_funded_and_future_loss_pays_fifo() {
        let mut s = state();
        let a = Pubkey::new_unique();
        let b = Pubkey::new_unique();
        ensure_user(&mut s, a).unwrap();
        ensure_user(&mut s, b).unwrap();
        s.positions.push(pos(1, a, 100_000_000, false));
        s.positions.push(pos(2, b, 100_000_000, false));
        let vault = expected_vault(&s).unwrap();
        settle_position(&mut s, 0, 101_000_000, 1, false).unwrap();
        assert_eq!(s.users[0].available, 100_000_000);
        assert_eq!(s.fee_team, 0);
        assert_eq!(s.claims.len(), 1);
        let debt_before = s.claims[0].amount;
        economic(&s);
        settle_position(&mut s, 0, 99_900_000, 2, false).unwrap();
        assert_eq!(s.users[0].available, 110_000_000);
        assert_eq!(s.users[1].available, 90_000_000);
        assert_eq!(s.claims[0].amount, debt_before - 10_000_000);
        assert_eq!(s.fee_team, 0);
        assert_eq!(s.users[1].token_credit, 1_000_000_000);
        assert_eq!(expected_vault(&s).unwrap(), vault);
        economic(&s);
    }
    #[test]
    fn debt_funded_bust_destroys_iou_and_mints_without_creating_cash() {
        let mut s = state();
        let a = Pubkey::new_unique();
        ensure_user(&mut s, a).unwrap();
        append_claim(&mut s, a, 60_000_000, 1).unwrap();
        s.positions.push(pos(1, a, 40_000_000, true));
        s.tracked_lp = -100_000_000;
        let vault = expected_vault(&s).unwrap();
        settle_position(&mut s, 0, 99_000_000, 2, true).unwrap();
        assert_eq!(s.claims[0].amount, 60_000_000);
        assert_eq!(s.users[0].token_credit, 4_000_000_000);
        assert_eq!(s.house_cash, 0);
        assert_eq!(s.fee_team, 0);
        assert_eq!(expected_vault(&s).unwrap(), vault);
        economic(&s);
    }
    #[test]
    fn partial_harvest_cannot_skip_oldest_owner() {
        let mut s = state();
        let a = Pubkey::new_unique();
        let b = Pubkey::new_unique();
        ensure_user(&mut s, a).unwrap();
        ensure_user(&mut s, b).unwrap();
        append_claim(&mut s, a, 100, 1).unwrap();
        append_claim(&mut s, b, 100, 2).unwrap();
        s.house_cash = 150;
        s.tracked_lp = -50;
        harvest(&mut s, 1).unwrap();
        assert_eq!(s.users[0].available, 100);
        assert_eq!(s.users[1].available, 0);
        assert_eq!(s.house_cash, 50);
        harvest(&mut s, 1).unwrap();
        assert_eq!(s.users[1].available, 50);
        assert_eq!(s.claims[0].amount, 50);
        economic(&s);
    }
    #[test]
    fn new_stake_does_not_capture_prior_fee_attribution() {
        let mut s = state();
        let a = Pubkey::new_unique();
        let b = Pubkey::new_unique();
        let ai = ensure_user(&mut s, a).unwrap();
        let bi = ensure_user(&mut s, b).unwrap();
        s.users[ai].stake = 100;
        s.total_stake = 100;
        s.house_cash = 100;
        s.tracked_lp = 100;
        accrue_fees(&mut s, 100).unwrap();
        settle_rewards(&mut s, bi).unwrap();
        s.users[bi].stake = 100;
        s.total_stake = 200;
        settle_rewards(&mut s, ai).unwrap();
        assert_eq!(s.users[ai].reward_credit, 50);
        assert_eq!(s.users[bi].reward_credit, 0);
        distribute(&mut s, 20).unwrap();
        settle_rewards(&mut s, ai).unwrap();
        settle_rewards(&mut s, bi).unwrap();
        assert_eq!(s.users[ai].reward_credit, 60);
        assert_eq!(s.users[bi].reward_credit, 10);
    }
    #[test]
    fn fees_without_stakers_remain_unassigned_until_public_flush() {
        let mut s = state();
        s.house_cash = 100;
        accrue_fees(&mut s, 100).unwrap();
        assert_eq!(s.reward_remainder, 50);
        assert_eq!(s.fee_staker, 50);
        let a = Pubkey::new_unique();
        let i = ensure_user(&mut s, a).unwrap();
        s.users[i].stake = 100;
        s.total_stake = 100;
        assert_eq!(s.users[i].reward_credit, 0);
        distribute(&mut s, 0).unwrap();
        settle_rewards(&mut s, i).unwrap();
        assert_eq!(s.reward_remainder, 0);
        assert_eq!(s.fee_staker, 50);
        assert_eq!(s.users[i].reward_credit, 50);
    }
    #[test]
    fn locked_debt_principal_blocks_fees_until_house_covers_it() {
        let mut s = state();
        let a = Pubkey::new_unique();
        ensure_user(&mut s, a).unwrap();
        s.positions.push(pos(1, a, 100_000_000, true));
        s.house_cash = 50_000_000;
        s.tracked_lp = -50_000_000;
        assert!(!can_accrue_fees(&s, 1_000_000).unwrap());
        s.house_cash = 101_000_000;
        assert!(can_accrue_fees(&s, 1_000_000).unwrap());
        s.house_cash = 100_000_000;
        assert!(!can_accrue_fees(&s, 1).unwrap());
        assert!(can_accrue_fees(&s, 0).unwrap());
    }
}
