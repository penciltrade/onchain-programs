use solana_program::program_error::ProgramError;
pub const USDC: u64 = 1_000_000;
pub const FLAT_THRESHOLD: i128 = 2_000_000 * USDC as i128;
pub const LP_PROFIT_BUFFER: i128 = 5_000 * USDC as i128;
/// The marginal share starts at 8%, then rises linearly from $100k to $1M.
pub const LP_CURVE_START: i128 = 100_000 * USDC as i128;
pub const LP_CURVE_END: i128 = 1_000_000 * USDC as i128;
pub const LP_STAKER_START_BPS: u128 = 800;
pub const LP_STAKER_SHARE_CAP_BPS: u128 = 9_900;
const LP_CURVE_SPAN: u128 = (LP_CURVE_END - LP_CURVE_START) as u128;
const LP_LOG_SCALE: u128 = 1_000_000_000_000_000_000;
const LN_2: u128 = 693_147_180_559_945_309;
/// Entry-normalized long notional. Rounding always increases the reserve.
pub const EXPOSURE_SCALE: u128 = 1_000_000_000_000;
pub const TAIL_SCALE: u128 = 120_000_000 * USDC as u128;
pub fn u64_checked(x: u128) -> Result<u64, ProgramError> {
    u64::try_from(x).map_err(|_| ProgramError::ArithmeticOverflow)
}
pub fn mul_div(a: u64, b: u64, d: u64) -> Result<u64, ProgramError> {
    if d == 0 {
        return Err(ProgramError::InvalidArgument);
    }
    u64_checked(a as u128 * b as u128 / d as u128)
}

// Fixed-point natural logarithm for ratios in [1,92]. Range reduction
// leaves y <= 1/3; 24 terms bound truncation well below one USDC atom.
fn lp_ln(mut ratio: u128) -> u128 {
    let mut powers = 0;
    while ratio >= 2 * LP_LOG_SCALE {
        ratio /= 2;
        powers += 1;
    }
    let y = (ratio - LP_LOG_SCALE) * LP_LOG_SCALE / (ratio + LP_LOG_SCALE);
    let square = y * y / LP_LOG_SCALE;
    let mut term = y;
    let mut sum = 0;
    for n in 0..24 {
        sum += term / (2 * n + 1);
        term = term * square / LP_LOG_SCALE;
    }
    2 * sum + powers * LN_2
}

// Integral of 1/(1-share(E)). Its inverse gives retained equity, so splitting
// a profit event cannot change its economic allocation except atom rounding.
fn lp_profit_potential(equity: u128) -> u128 {
    let below = equity.min(LP_CURVE_START as u128) - LP_PROFIT_BUFFER as u128;
    let early = below * 10_000 / (10_000 - LP_STAKER_START_BPS);
    let x = equity
        .saturating_sub(LP_CURVE_START as u128)
        .min(LP_CURVE_SPAN);
    let denominator = 9_200 * LP_CURVE_SPAN - 9_100 * x;
    let ratio = 9_200 * LP_CURVE_SPAN * LP_LOG_SCALE / denominator;
    let rising = LP_CURVE_SPAN * 10_000 * lp_ln(ratio) / (9_100 * LP_LOG_SCALE);
    early + rising + equity.saturating_sub(LP_CURVE_END as u128) * 100
}

// A lower bound on exp(-x), x <= ln(92). Reduce to <=1/8, use the
// odd Taylor partial sum, subtract a bound for integer rounding, then square.
fn lp_exp_neg_lower(mut x: u128) -> u128 {
    let mut powers = 0;
    while x > LP_LOG_SCALE / 8 {
        x = (x + 1) / 2;
        powers += 1;
    }
    let mut term = LP_LOG_SCALE;
    let mut sum = LP_LOG_SCALE as i128;
    for n in 1..=19 {
        term = term * x / (LP_LOG_SCALE * n);
        if n % 2 == 1 {
            sum -= term as i128;
        } else {
            sum += term as i128;
        }
    }
    let mut result = (sum - 32).max(0) as u128;
    for _ in 0..powers {
        result = result * result / LP_LOG_SCALE;
    }
    result
}

pub fn lp_staker_share_bps(equity: i128) -> Result<u64, ProgramError> {
    if equity < LP_PROFIT_BUFFER {
        return Ok(0);
    }
    u64_checked(equity as u128)?;
    let x = equity.saturating_sub(LP_CURVE_START).max(0) as u128;
    Ok((LP_STAKER_START_BPS
        + (LP_STAKER_SHARE_CAP_BPS - LP_STAKER_START_BPS) * x.min(LP_CURVE_SPAN) / LP_CURVE_SPAN)
        as u64)
}

/// Integrate the 8%-99% marginal share over new, funded profit. Repay the
/// deficit and $5k buffer first. Existing capital never enters the split.
pub fn lp_profit_share(before: i128, profit: u64) -> Result<u64, ProgramError> {
    let bootstrap =
        (LP_PROFIT_BUFFER.saturating_sub(before).max(0) as u128).min(profit as u128) as u64;
    let shareable = profit - bootstrap;
    if shareable == 0 {
        return Ok(0);
    }
    let start = before.max(LP_PROFIT_BUFFER) as u128;
    u64_checked(start)?;
    if start >= LP_CURVE_END as u128 {
        return Ok(shareable - ((shareable as u128 + 99) / 100) as u64);
    }
    let initial = lp_profit_potential(start);
    let cap_cost = lp_profit_potential(LP_CURVE_END as u128) - initial;
    if shareable as u128 >= cap_cost {
        let retained = LP_CURVE_END as u128 - start + (shareable as u128 - cap_cost + 99) / 100;
        return Ok(shareable - u64_checked(retained)?);
    }
    let mut retained = 0u128;
    let mut remaining = shareable as u128;
    let mut curve_start = start;
    if start < LP_CURVE_START as u128 {
        let early_cost = lp_profit_potential(LP_CURVE_START as u128) - initial;
        if remaining <= early_cost {
            // Invert floor((E-buffer)*10000/9200) exactly.
            let target = initial + remaining;
            retained = (target * 9_200 + 9_999) / 10_000 + LP_PROFIT_BUFFER as u128 - start;
            return Ok(shareable - u64_checked(retained)?);
        }
        retained = LP_CURVE_START as u128 - start;
        remaining -= early_cost;
        curve_start = LP_CURVE_START as u128;
    }
    // Closed-form inverse of the logarithmic integral avoids a binary search
    // (and repeated logarithms) on the compute-limited execution path.
    let x = curve_start - LP_CURVE_START as u128;
    let retention_numerator = 9_200 * LP_CURVE_SPAN - 9_100 * x;
    let exponent_numerator = remaining * 9_100 * LP_LOG_SCALE;
    let exponent_denominator = 10_000 * LP_CURVE_SPAN;
    let exponent = (exponent_numerator + exponent_denominator - 1) / exponent_denominator;
    let exponential = lp_exp_neg_lower(exponent);
    let numerator = retention_numerator * (LP_LOG_SCALE - exponential);
    let denominator = 9_100 * LP_LOG_SCALE;
    retained += (numerator + denominator - 1) / denominator;
    Ok(shareable - u64_checked(retained.min(shareable as u128))?)
}

/// Upper bound on gross winning profit (net payout plus trading fee).
/// A short can win at most 90% of notional. A long is bounded by 90% of
/// price * notional/entry; omitting the entry deduction over-reserves cash.
pub fn winning_reserve(
    long_units: u128,
    short_notional: u128,
    mark: u64,
) -> Result<u64, ProgramError> {
    if long_units == 0 && short_notional == 0 {
        return Ok(0);
    }
    if long_units > 0 && mark == 0 {
        return Ok(u64::MAX);
    }
    // Cover the largest next accepted mark as well as the current mark.
    let next_mark = (mark as u128 * 13 + 9) / 10;
    let numerator = Wide::product(long_units, next_mark.min(u64::MAX as u128));
    let mut longs = numerator.divide(Wide::from_u128(EXPOSURE_SCALE))?;
    // Floor plus one is conservative, including at exact divisibility.
    if longs.at_least(Wide::from_u128(u64::MAX as u128)) {
        return Ok(u64::MAX);
    }
    let total = longs
        .as_u128()?
        .saturating_add((long_units > 0) as u128)
        .saturating_add(short_notional);
    // Ceil(0.9 * total). Clamp a reserve larger than all possible house cash.
    longs = Wide::product(total, 9).divide(Wide::from_u128(10))?;
    if longs.at_least(Wide::from_u128(u64::MAX as u128)) {
        return Ok(u64::MAX);
    }
    Ok(longs.as_u128()?.saturating_add(1).min(u64::MAX as u128) as u64)
}

// Wide intermediates avoid spurious overflow in multiply-before-divide ratios.
// No float arithmetic, heap allocation, or big-integer dependency is required.
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
struct Wide([u64; 4]);
impl Wide {
    fn from_u128(v: u128) -> Self {
        Self([v as u64, (v >> 64) as u64, 0, 0])
    }
    fn product(a: u128, b: u128) -> Self {
        let aa = [a as u64, (a >> 64) as u64];
        let bb = [b as u64, (b >> 64) as u64];
        let mut out = [0; 4];
        for i in 0..2 {
            let mut carry = 0u128;
            for j in 0..2 {
                let value = aa[i] as u128 * bb[j] as u128 + out[i + j] as u128 + carry;
                out[i + j] = value as u64;
                carry = value >> 64;
            }
            out[i + 2] = carry as u64;
        }
        Self(out)
    }
    fn times_small(mut self, m: u64) -> Result<Self, ProgramError> {
        let mut carry = 0u128;
        for limb in &mut self.0 {
            let value = *limb as u128 * m as u128 + carry;
            *limb = value as u64;
            carry = value >> 64;
        }
        if carry != 0 {
            return Err(ProgramError::ArithmeticOverflow);
        }
        Ok(self)
    }
    fn at_least(self, rhs: Self) -> bool {
        for i in (0..4).rev() {
            if self.0[i] != rhs.0[i] {
                return self.0[i] > rhs.0[i];
            }
        }
        true
    }
    fn subtract_wrapping(mut self, rhs: Self) -> Self {
        let mut borrow = false;
        for i in 0..4 {
            let (first, b1) = self.0[i].overflowing_sub(rhs.0[i]);
            let (second, b2) = first.overflowing_sub(borrow as u64);
            self.0[i] = second;
            borrow = b1 || b2;
        }
        self
    }
    fn divide(self, divisor: Self) -> Result<Self, ProgramError> {
        if divisor == Self::default() {
            return Err(ProgramError::InvalidArgument);
        }
        let mut q = Self::default();
        let mut r = Self::default();
        for bit in (0..256).rev() {
            let carry = r.0[3] >> 63 != 0;
            for i in (1..4).rev() {
                r.0[i] = (r.0[i] << 1) | (r.0[i - 1] >> 63);
            }
            r.0[0] = (r.0[0] << 1) | ((self.0[bit / 64] >> (bit % 64)) & 1);
            // If shifted remainder has a 257th bit, wrapped subtraction removes it.
            if carry || r.at_least(divisor) {
                r = r.subtract_wrapping(divisor);
                q.0[bit / 64] |= 1u64 << (bit % 64);
            }
        }
        Ok(q)
    }
    fn as_u128(self) -> Result<u128, ProgramError> {
        if self.0[2] != 0 || self.0[3] != 0 {
            return Err(ProgramError::ArithmeticOverflow);
        }
        Ok(self.0[0] as u128 | ((self.0[1] as u128) << 64))
    }
}

pub fn busted(entry: u64, exit: u64, side: i8, leverage: u16) -> bool {
    // Invalid terms fail closed. Five basis points of entry-price cushion.
    if entry == 0 || exit == 0 || !matches!(side, -1 | 1) || !(1..=200).contains(&leverage) {
        return true;
    }
    let adverse = if side == 1 {
        entry.saturating_sub(exit)
    } else {
        exit.saturating_sub(entry)
    };
    adverse as u128 * leverage as u128 * 10_000 >= entry as u128 * (10_000 - 5 * leverage as u128)
}
pub fn pnl(
    margin: u64,
    leverage: u16,
    entry: u64,
    exit: u64,
    side: i8,
    pm: u64,
) -> Result<(bool, u64, u64), ProgramError> {
    if entry == 0
        || exit == 0
        || pm == 0
        || !matches!(side, -1 | 1)
        || !(1..=200).contains(&leverage)
    {
        return Err(ProgramError::InvalidArgument);
    }
    let favorable = (side == 1 && exit >= entry) || (side == -1 && exit <= entry);
    let diff = entry.abs_diff(exit);
    let notional = margin as u128 * leverage as u128;
    if !favorable {
        // Cap before narrowing: even an extreme losing price loses only margin.
        let loss = Wide::product(notional, diff as u128).divide(Wide::from_u128(entry as u128))?;
        let capped = if loss.at_least(Wide::from_u128(margin as u128)) {
            margin
        } else {
            u64_checked(loss.as_u128()?)?
        };
        return Ok((false, capped, 0));
    }
    // Effective move is |exit-entry|/entry - 1/50000. Only raw USDC gain rounds
    // down before the haircut, costing less than one atomic USDC of precision.
    let effective = (diff as u128 * 50_000).saturating_sub(entry as u128);
    if effective == 0 {
        return Ok((true, 0, 0));
    }
    let denominator_price = entry as u128 * 50_000;
    let raw = Wide::product(notional, effective)
        .divide(Wide::from_u128(denominator_price))?
        .as_u128()?;
    // pm is positionMultiplier in milli-units: BTC=814598, ETH=483979.
    // scale=.9/(1+1/(move*15000)+.1/(move*(pm/1000))).
    let move_term = effective
        .checked_mul(15_000)
        .and_then(|x| x.checked_mul(pm as u128))
        .ok_or(ProgramError::ArithmeticOverflow)?;
    let denom = move_term
        .checked_add(
            denominator_price
                .checked_mul(pm as u128 + 1_500_000)
                .ok_or(ProgramError::ArithmeticOverflow)?,
        )
        .ok_or(ProgramError::ArithmeticOverflow)?;
    let adjusted = Wide::product(raw, move_term)
        .times_small(9)?
        .divide(Wide::from_u128(denom).times_small(10)?)?
        .as_u128()?;
    let fee = adjusted / 50;
    Ok((true, u64_checked(adjusted - fee)?, u64_checked(fee)?))
}
pub fn emission(
    basis: u64,
    lp_gain: u64,
    tracked_before: i128,
    highwater: &mut u64,
) -> Result<u64, ProgramError> {
    // Integrate the curve through a loss, splitting at $2M and apportioning basis
    // by LP gain. Below $2M the original flat branch applies again; the tail's
    // high-water accumulator is retained and never decreases.
    if basis == 0 {
        return Ok(0);
    }
    let flat_gain = if tracked_before < FLAT_THRESHOLD {
        FLAT_THRESHOLD
            .saturating_sub(tracked_before)
            .min(lp_gain as i128) as u64
    } else {
        0
    };
    emission_split(basis, lp_gain, tracked_before, flat_gain, highwater)
}

/// `share_from` is the equity level at which this gain starts to be shared:
/// the settlement's opening equity, or the LP high-water mark when that is
/// higher. Equity below it is regained one for one.
pub fn emission_with_lp_profit_share(
    basis: u64,
    lp_gain: u64,
    tracked_before: i128,
    share_from: i128,
    highwater: &mut u64,
) -> Result<u64, ProgramError> {
    let flat_gain = if tracked_before >= FLAT_THRESHOLD {
        0
    } else {
        let base = share_from.max(tracked_before).min(FLAT_THRESHOLD);
        // Unshared regain up to the base, then the integrated profit cost of
        // lifting actual equity from the base to the flat threshold.
        let unshared = base.saturating_sub(tracked_before).max(0) as u128;
        let cost = if base >= FLAT_THRESHOLD {
            0
        } else {
            let bootstrap = LP_PROFIT_BUFFER.saturating_sub(base).max(0) as u128;
            let start = base.max(LP_PROFIT_BUFFER) as u128;
            let end = FLAT_THRESHOLD as u128;
            bootstrap + lp_profit_potential(end) - lp_profit_potential(start)
        };
        unshared.saturating_add(cost).min(lp_gain as u128) as u64
    };
    // Tail progress remains gross eligible LP gain, as before. The allocation
    // curve changes how much profit is needed to reach actual $2M equity.
    emission_split(basis, lp_gain, tracked_before, flat_gain, highwater)
}

fn emission_split(
    basis: u64,
    lp_gain: u64,
    tracked_before: i128,
    flat_gain: u64,
    highwater: &mut u64,
) -> Result<u64, ProgramError> {
    if basis == 0 {
        return Ok(0);
    }
    let flat_basis = if lp_gain == 0 {
        if tracked_before < FLAT_THRESHOLD {
            basis
        } else {
            0
        }
    } else {
        mul_div(basis, flat_gain, lp_gain)?
    };
    let tail_basis = basis - flat_basis;
    let tail_gain = lp_gain - flat_gain;
    let next_highwater = highwater
        .checked_add(tail_gain)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    let a = TAIL_SCALE + *highwater as u128;
    let b = TAIL_SCALE + next_highwater as u128;
    let tail = Wide::product(tail_basis as u128, TAIL_SCALE * TAIL_SCALE)
        .times_small(100)?
        .divide(Wide::product(a, b))?
        .as_u128()?;
    let minted = u64_checked(flat_basis as u128 * 100 + tail)?;
    // Preserve caller state if arithmetic cannot be represented.
    *highwater = next_highwater;
    Ok(minted)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lp_share_has_selected_anchors_and_monotone_bounds() {
        for (equity, bps) in [
            (0, 0),
            (4999, 0),
            (5000, 800),
            (100000, 800),
            (550000, 5350),
            (1000000, 9900),
            (200000000, 9900),
        ] {
            assert_eq!(lp_staker_share_bps(equity * USDC as i128).unwrap(), bps);
        }
        let mut last = 0;
        for equity in (0..=2_000_000).step_by(1000) {
            let rate = lp_staker_share_bps(equity * USDC as i128).unwrap();
            assert!(rate >= last && rate <= 9900);
            last = rate;
        }
        assert_eq!(lp_staker_share_bps(u64::MAX as i128).unwrap(), 9900);
    }
    #[test]
    fn lp_profit_refills_buffer_and_never_exports_existing_capital() {
        assert_eq!(
            lp_profit_share(-100 * USDC as i128, 5100 * USDC).unwrap(),
            0
        );
        assert_eq!(lp_profit_share(0, 5000 * USDC).unwrap(), 0);
        assert_eq!(lp_profit_share(0, 6000 * USDC).unwrap(), 80 * USDC);
        assert_eq!(lp_profit_share(i128::MIN, u64::MAX).unwrap(), 0);
        assert_eq!(lp_profit_share(10_000_000 * USDC as i128, 0).unwrap(), 0);
        assert_eq!(
            lp_profit_share(LP_PROFIT_BUFFER, 100 * USDC).unwrap(),
            8 * USDC
        );
        assert_eq!(
            lp_profit_share(LP_CURVE_END, 100 * USDC).unwrap(),
            99 * USDC
        );
        assert_eq!(
            lp_profit_share(LP_CURVE_END, 480 * USDC).unwrap(),
            475_200_000
        );
    }
    #[test]
    fn lp_profit_integral_matches_independent_logarithmic_reference() {
        let span = 900_000.0 * USDC as f64;
        let beta = 0.91 / span;
        for before in [5000, 30000, 100000, 550000, 999999, 1000000, 5000000] {
            for profit in [1, 100, 10000, 1000000, 100000000] {
                let mut p = profit as f64 * USDC as f64;
                let mut equity = before as f64 * USDC as f64;
                let start = equity;
                if equity < 100000.0 * USDC as f64 {
                    let retained = (100000.0 * USDC as f64 - equity).min(p * 0.92);
                    equity += retained;
                    p -= retained / 0.92;
                }
                if p > 0.0 && equity < 1000000.0 * USDC as f64 {
                    let retention = 0.92 - beta * (equity - 100000.0 * USDC as f64);
                    let cost = (retention / 0.01).ln() / beta;
                    let used = p.min(cost);
                    equity += retention * (-(-beta * used).exp_m1()) / beta;
                    p -= used;
                }
                equity += p * 0.01;
                let got =
                    profit * USDC - lp_profit_share(before * USDC as i128, profit * USDC).unwrap();
                assert!(
                    (got as f64 - (equity - start)).abs() < 3.0,
                    "before={before},profit={profit},got={got},reference={}",
                    equity - start
                );
            }
        }
    }
    #[test]
    fn lp_allocation_splitting_changes_only_atomic_rounding() {
        for before in [
            0, 1000, 5000, 100000, 550000, 999999, 1000000, 5000000, 200000000,
        ] {
            let initial = before * USDC as i128;
            let profit = 6_000_000 * USDC;
            let whole = profit - lp_profit_share(initial, profit).unwrap();
            let mut split = 0;
            for _ in 0..100 {
                let part = profit / 100;
                split += part - lp_profit_share(initial + split as i128, part).unwrap();
            }
            assert!(
                whole.abs_diff(split) < 300,
                "before={before},whole={whole},split={split}"
            );
        }
    }
    #[test]
    fn lp_maximum_profit_and_retention_floor_are_safe() {
        let before = 200_000_000 * USDC as i128;
        assert_eq!(lp_profit_share(before, 100 * USDC).unwrap(), 99 * USDC);
        let share = lp_profit_share(before, u64::MAX).unwrap();
        let kept = (u64::MAX - share) as u128;
        let exact = (u64::MAX as u128 + 99) / 100;
        assert_eq!(kept, exact);
        assert_eq!(winning_reserve(0, 0, 0).unwrap(), 0);
        assert_eq!(winning_reserve(1, 0, 0).unwrap(), u64::MAX);
        assert_eq!(winning_reserve(u128::MAX, 0, u64::MAX).unwrap(), u64::MAX);
        assert_eq!(winning_reserve(0, u128::MAX, 1).unwrap(), u64::MAX);
    }
    #[test]
    fn lp_emission_crossing_uses_actual_equity_without_event_size_bonus() {
        let initial = LP_PROFIT_BUFFER;
        let profit = 110_000_000 * USDC;
        let mut whole_h = 0;
        let whole =
            emission_with_lp_profit_share(profit, profit, initial, initial, &mut whole_h).unwrap();
        let mut equity = initial;
        let mut split_h = 0;
        let mut split = 0u64;
        for _ in 0..100 {
            let part = profit / 100;
            split +=
                emission_with_lp_profit_share(part, part, equity, equity, &mut split_h).unwrap();
            equity += (part - lp_profit_share(equity, part).unwrap()) as i128;
        }
        assert!(whole_h.abs_diff(split_h) < 900);
        assert!(whole.abs_diff(split) < 90_100);
        assert!(equity > FLAT_THRESHOLD);
        let below_profit = 2_100_000 * USDC;
        let mut h = 0;
        assert_eq!(
            emission_with_lp_profit_share(below_profit, below_profit, initial, initial, &mut h)
                .unwrap(),
            below_profit * 100
        );
        assert_eq!(h, 0, "Sharing keeps actual equity below $2M at this profit");
    }
    #[test]
    fn one_percent_move_preserves_selected_asymmetric_payouts() {
        assert_eq!(
            pnl(10_000 * USDC, 10, 80_000 * USDC, 80_800 * USDC, 1, 814598).unwrap(),
            (true, 863_839_784, 17_629_383)
        );
        assert_eq!(
            pnl(10_000 * USDC, 10, 80_000 * USDC, 80_800 * USDC, 1, 483979).unwrap(),
            (true, 856_774_526, 17_485_194)
        );
        assert_eq!(
            pnl(10_000 * USDC, 10, 80_000 * USDC, 79_200 * USDC, 1, 814598).unwrap(),
            (false, 1_000 * USDC, 0)
        );
    }
    fn reference(margin: u64, lev: u16, entry: u64, exit: u64, pm: u64) -> (u64, u64) {
        let m = (entry.abs_diff(exit) as f64 / entry as f64 - 1.0 / 50000.0).max(0.0);
        if m == 0.0 {
            return (0, 0);
        }
        let scale = 0.9 / (1.0 + 1.0 / (m * 15000.0) + 0.1 / (m * (pm as f64 / 1000.0)));
        let adjusted = (margin as f64 * lev as f64 * m * scale).floor() as u64;
        (adjusted - adjusted / 50, adjusted / 50)
    }
    #[test]
    fn wide_arithmetic_reference_and_boundaries() {
        let values = [
            0u128,
            1,
            9,
            u64::MAX as u128,
            u64::MAX as u128 + 1,
            u128::MAX / 3,
            u128::MAX,
        ];
        for &a in &values {
            for &b in &values {
                if let Some(p) = a.checked_mul(b) {
                    assert_eq!(Wide::product(a, b).as_u128().unwrap(), p);
                }
                if a != 0 {
                    assert_eq!(
                        Wide::product(a, b)
                            .divide(Wide::from_u128(a))
                            .unwrap()
                            .as_u128()
                            .unwrap(),
                        b
                    );
                }
            }
        }
        let largest = Wide::product(u128::MAX, u128::MAX);
        assert_eq!(largest.0, [1, 0, u64::MAX - 1, u64::MAX]);
        assert_eq!(largest.divide(largest).unwrap(), Wide::from_u128(1));
        assert!(Wide::from_u128(1).divide(Wide::default()).is_err());
        assert!(Wide([u64::MAX; 4]).times_small(2).is_err());
    }
    #[test]
    fn hard_bust_precedes_zero_on_each_selected_leverage() {
        for leverage in [1, 10, 100, 200] {
            let entry = 2_000_000u64;
            let adverse = entry / leverage as u64 - entry / 2000;
            assert!(!busted(entry, entry - adverse + 1, 1, leverage));
            assert!(busted(entry, entry - adverse, 1, leverage));
            assert!(!busted(entry, entry + adverse - 1, -1, leverage));
            assert!(busted(entry, entry + adverse, -1, leverage));
        }
        assert!(busted(100, 100, 0, 200));
        assert!(busted(100, 100, 1, u16::MAX));
    }
    #[test]
    fn haircut_matches_original_independent_reference() {
        for pm in [814598, 483979] {
            let mut previous = 0;
            for bps in [1, 2, 5, 10, 50, 100, 1000] {
                let entry = 100_000_000;
                let exit = entry + entry * bps / 10_000;
                let (_, net, fee) = pnl(100 * USDC, 100, entry, exit, 1, pm).unwrap();
                let expected = reference(100 * USDC, 100, entry, exit, pm);
                assert!(net.abs_diff(expected.0) <= 2, "pm={pm},bps={bps}");
                assert!(fee.abs_diff(expected.1) <= 1);
                assert!(net > previous);
                previous = net;
                assert_eq!(
                    pnl(100 * USDC, 100, entry, entry - entry * bps / 10_000, -1, pm).unwrap(),
                    (true, net, fee)
                );
                let doubled = pnl(200 * USDC, 100, entry, exit, 1, pm).unwrap();
                assert!(doubled.1.abs_diff(2 * net) <= 2);
                assert!(
                    (net as u128 + fee as u128) * 10
                        <= (100 * USDC as u128) * 100 * bps as u128 / 10_000 * 9
                );
            }
        }
        assert_eq!(
            pnl(100 * USDC, 100, 100_000_000, 100_002_000, 1, 814598).unwrap(),
            (true, 0, 0)
        );
        assert_eq!(
            pnl(100 * USDC, 100, 100_000_000, 100_001_999, 1, 814598).unwrap(),
            (true, 0, 0)
        );
    }
    #[test]
    fn large_valid_trade_and_extreme_loss_do_not_overflow() {
        for (entry, pm) in [
            (1_800 * USDC, 483979),
            (3_000 * USDC, 483979),
            (83_000 * USDC, 814598),
            (120_000 * USDC, 814598),
        ] {
            for margin in [USDC, 100 * USDC, 50_000 * USDC] {
                for lev in [1, 25, 100, 200] {
                    for bps in [1, 5, 10, 50, 100, 1000] {
                        let exit = entry + entry * bps / 10_000;
                        let (_, net, fee) = pnl(margin, lev, entry, exit, 1, pm).unwrap();
                        let expected = reference(margin, lev, entry, exit, pm);
                        assert!(net.abs_diff(expected.0) <= 2);
                        assert!(fee.abs_diff(expected.1) <= 1);
                    }
                }
            }
        }
        let entry = 10_000_000_000_000_000_000;
        let (_, net, fee) = pnl(50_000 * USDC, 200, entry, entry + entry / 100, 1, 814598).unwrap();
        assert!(net > 86_000 * USDC && net < 87_000 * USDC);
        assert!(fee > 1700 * USDC && fee < 1800 * USDC);
        assert_eq!(
            pnl(u64::MAX, 200, 1, u64::MAX, -1, 814598).unwrap(),
            (false, u64::MAX, 0)
        );
    }
    #[test]
    fn invalid_terms_and_unrepresentable_profit_rejected() {
        for (entry, exit, side, lev, pm) in [
            (0, 1, 1, 1, 1),
            (1, 0, 1, 1, 1),
            (1, 1, 0, 1, 1),
            (1, 1, 1, 0, 1),
            (1, 1, 1, 201, 1),
            (1, 1, 1, 1, 0),
        ] {
            assert!(pnl(USDC, lev, entry, exit, side, pm).is_err());
        }
        assert!(pnl(u64::MAX, 200, 1, u64::MAX, 1, 814598).is_err());
    }
    #[test]
    fn mint_curve_crossing_and_irreversible_highwater() {
        let mut h = 0;
        assert_eq!(emission(USDC, USDC, 0, &mut h).unwrap(), 100 * USDC);
        let mut cross_h = 0;
        let crossed = emission(
            2 * USDC,
            2 * USDC,
            FLAT_THRESHOLD - USDC as i128,
            &mut cross_h,
        )
        .unwrap();
        assert_eq!(cross_h, USDC);
        assert_eq!(
            crossed,
            100 * USDC + (100u128 * USDC as u128 * TAIL_SCALE / (TAIL_SCALE + USDC as u128)) as u64
        );
        let minted =
            emission(10_000_000 * USDC, 10_000_000 * USDC, FLAT_THRESHOLD, &mut h).unwrap();
        assert_eq!(
            minted,
            (1_000_000_000u128 * USDC as u128 * 120 / 130) as u64
        );
        assert_eq!(h, 10_000_000 * USDC);
        let previous = h;
        assert_eq!(emission(USDC, USDC, i128::MIN, &mut h).unwrap(), 100 * USDC);
        assert_eq!(h, previous);
        assert!(emission(USDC, USDC, FLAT_THRESHOLD, &mut h).unwrap() < 100 * USDC);
    }
    #[test]
    fn tail_mint_batch_invariance_and_error_atomicity() {
        let mut single_h = 0;
        let single = emission(
            10_000_000 * USDC,
            10_000_000 * USDC,
            FLAT_THRESHOLD,
            &mut single_h,
        )
        .unwrap();
        let mut split_h = 0;
        let mut split = 0;
        for _ in 0..10 {
            split += emission(
                1_000_000 * USDC,
                1_000_000 * USDC,
                FLAT_THRESHOLD,
                &mut split_h,
            )
            .unwrap();
        }
        assert_eq!(single_h, split_h);
        assert!(single >= split && single - split <= 9);
        let mut h = u64::MAX;
        assert!(emission(USDC, USDC, FLAT_THRESHOLD, &mut h).is_err());
        assert_eq!(h, u64::MAX);
        let mut flat_h = 0;
        assert!(emission(u64::MAX, 0, 0, &mut flat_h).is_err());
        assert_eq!(flat_h, 0);
    }
}
