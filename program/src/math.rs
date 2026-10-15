use solana_program::program_error::ProgramError;
pub const USDC: u64 = 1_000_000;
pub const FLAT_THRESHOLD: i128 = 500_000 * USDC as i128;
pub const LP_PROFIT_BUFFER: i128 = 5_000 * USDC as i128;
/// The marginal share follows 8% + 32% * (equity / $1M)^2.
pub const LP_CURVE_START: i128 = 0;
pub const LP_CURVE_END: i128 = 1_000_000 * USDC as i128;
pub const LP_STAKER_START_BPS: u128 = 800;
pub const LP_STAKER_SHARE_CAP_BPS: u128 = 4_000;
const LP_SQRT_RATIO: u128 = 589_767_824_619_588_533; // floor(sqrt(8/23) * 1e18)
const LP_LOG_SCALE: u128 = 1_000_000_000_000_000_000;
const LN_2: u128 = 693_147_180_559_945_309;
/// Entry-normalized long notional. Rounding always increases the reserve.
pub const EXPOSURE_SCALE: u128 = 1_000_000_000_000;
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
#[inline(never)]
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
#[inline(never)]
fn lp_profit_potential(equity: u128) -> u128 {
    let capped = equity.min(LP_CURVE_END as u128);
    let x = LP_SQRT_RATIO * capped / LP_CURVE_END as u128;
    let ratio = (LP_LOG_SCALE + x) * LP_LOG_SCALE / (LP_LOG_SCALE - x);
    // Integral of 1/(.92-.32*(E/M)^2), anchored at zero.
    LP_CURVE_END as u128 * 25 * lp_ln(ratio) / (46 * LP_SQRT_RATIO)
        + equity.saturating_sub(LP_CURVE_END as u128) * 5 / 3
}

// A lower bound on exp(-x), x <= ln(92). Reduce to <=1/8, use the
// odd Taylor partial sum, subtract a bound for integer rounding, then square.
#[inline(never)]
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
    let x = (equity as u128).min(LP_CURVE_END as u128);
    Ok((LP_STAKER_START_BPS
        + (LP_STAKER_SHARE_CAP_BPS - LP_STAKER_START_BPS) * x * x
            / (LP_CURVE_END as u128 * LP_CURVE_END as u128)) as u64)
}

/// Integrate the quadratic share over funded profit after the $5k buffer.
/// Closed-form inversion bounds work independently of profit and LP size.
#[inline(never)]
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
        return Ok(shareable - ((shareable as u128 * 3 + 4) / 5) as u64);
    }
    let cost = lp_profit_potential(LP_CURVE_END as u128) - lp_profit_potential(start);
    if shareable as u128 >= cost {
        let retained = LP_CURVE_END as u128 - start + ((shareable as u128 - cost) * 3 + 4) / 5;
        return Ok(shareable - u64_checked(retained.min(shareable as u128))?);
    }
    let x = LP_SQRT_RATIO * start / LP_CURVE_END as u128;
    let denominator = 25 * LP_CURVE_END as u128;
    let exponent = (shareable as u128 * 46 * LP_SQRT_RATIO + denominator - 1) / denominator;
    let exponential = lp_exp_neg_lower(exponent);
    let denominator = LP_LOG_SCALE + x + (LP_LOG_SCALE - x) * exponential / LP_LOG_SCALE;
    let numerator = (LP_LOG_SCALE - x * x / LP_LOG_SCALE) * (LP_LOG_SCALE - exponential);
    let delta = (numerator + denominator - 1) / denominator;
    // One additional atom covers fixed-point error in the inverse. Rounding
    // retains cash; it cannot export existing capital or exceed funded profit.
    let retained = ((LP_CURVE_END as u128 * delta + LP_SQRT_RATIO - 1) / LP_SQRT_RATIO + 1)
        .min(shareable as u128);
    Ok(shareable - u64_checked(retained)?)
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
// 16-point Gauss-Legendre nodes and weights on [0,1], scaled by 1e18.
// Split at the flat threshold and LP cap before quadrature. On each smooth
// interval the nearest pole is outside the interval. No float runs onchain.
const GAUSS: [(u128, u128); 16] = [
    (5299532504175033, 13576229705877047),
    (27712488463383711, 31126761969323946),
    (67184398806084128, 47579255841246392),
    (122297795822498483, 62314485627766936),
    (191061877798678125, 74797994408288366),
    (270991611171386306, 84578259697501269),
    (359198224610370543, 91301707522461794),
    (452493745081181279, 94725305227534248),
    (547506254918818720, 94725305227534248),
    (640801775389629456, 91301707522461794),
    (729008388828613693, 84578259697501269),
    (808938122201321874, 74797994408288366),
    (877702204177501516, 62314485627766936),
    (932815601193915871, 47579255841246392),
    (972287511536616288, 31126761969323946),
    (994700467495824966, 13576229705877051),
];

#[inline(never)]
fn plain_integral(start: i128, gain: u64) -> Result<u128, ProgramError> {
    let flat = if start < FLAT_THRESHOLD {
        FLAT_THRESHOLD.saturating_sub(start).min(gain as i128) as u64
    } else {
        0
    };
    // An all-flat event uses the direct rate branch, including i128::MIN.
    let flat_part = (flat as u128)
        .checked_mul(100 * LP_LOG_SCALE)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    let delta = (gain - flat) as u128;
    if delta == 0 {
        return Ok(flat_part);
    }
    let a = start.max(FLAT_THRESHOLD) as u128;
    u64_checked(a)?;
    let c = FLAT_THRESHOLD as u128 * 100;
    let tail = if delta <= a / 1_000_000 {
        // Stable for tiny changes at large LP. Integrated midpoint error is
        // less than C * (delta/a)^3 / 12, below 0.000005 credit atoms.
        delta * (2 * c * LP_LOG_SCALE / (2 * a + delta))
    } else {
        c * lp_ln((a + delta) * LP_LOG_SCALE / a)
    };
    flat_part
        .checked_add(tail)
        .ok_or(ProgramError::ArithmeticOverflow)
}

#[inline(never)]
fn curve_integrals(start: u128, end: u128, offset: u128) -> (u128, u128) {
    let width = end - start;
    let mut cost = 0;
    let mut reward = 0;
    for (node, weight) in GAUSS {
        let x = start * LP_LOG_SCALE / LP_CURVE_END as u128 + width * node / LP_CURVE_END as u128;
        let retention = (9_200 * LP_LOG_SCALE - 3_200 * (x * x / LP_LOG_SCALE)) / 10_000;
        let inverse = LP_LOG_SCALE * LP_LOG_SCALE / retention;
        let tracked = offset * LP_LOG_SCALE / LP_CURVE_END as u128 + x;
        let rate = if tracked <= LP_LOG_SCALE / 2 {
            100 * LP_LOG_SCALE
        } else {
            50 * LP_LOG_SCALE * LP_LOG_SCALE / tracked
        };
        cost += inverse * weight / LP_LOG_SCALE;
        reward += (rate * LP_LOG_SCALE / retention) * weight / LP_LOG_SCALE;
    }
    (width * cost, width * reward)
}

#[inline(never)]
fn shared_average(start: u128, end: u128, offset: u128) -> Result<u128, ProgramError> {
    if start == end {
        return Ok(100 * LP_LOG_SCALE * FLAT_THRESHOLD as u128
            / (start + offset).max(FLAT_THRESHOLD as u128));
    }
    let mut at = start;
    let mut cost = 0u128;
    let mut reward = 0u128;
    if at < LP_PROFIT_BUFFER as u128 {
        let next = end.min(LP_PROFIT_BUFFER as u128);
        let width = u64_checked(next - at)?;
        cost += width as u128 * LP_LOG_SCALE;
        reward += plain_integral(
            i128::try_from(at + offset).map_err(|_| ProgramError::ArithmeticOverflow)?,
            width,
        )?;
        at = next;
    }
    // Split the only kink inside the quadratic interval.
    let flat_end = (FLAT_THRESHOLD as u128).saturating_sub(offset);
    for boundary in [flat_end.min(LP_CURVE_END as u128), LP_CURVE_END as u128] {
        let next = end.min(boundary);
        if next > at {
            let (c, r) = curve_integrals(at, next, offset);
            cost += c;
            reward += r;
            at = next;
        }
    }
    if end > at {
        let width = u64_checked(end - at)?;
        cost += width as u128 * LP_LOG_SCALE * 5 / 3;
        reward += plain_integral(
            i128::try_from(at + offset).map_err(|_| ProgramError::ArithmeticOverflow)?,
            width,
        )? * 5
            / 3;
    }
    Wide::product(reward, LP_LOG_SCALE)
        .divide(Wide::from_u128(cost))?
        .as_u128()
}

pub fn emission(
    basis: u64,
    lp_gain: u64,
    before: i128,
    legacy: &mut u64,
) -> Result<u64, ProgramError> {
    emission_with_allocation(basis, lp_gain, before, None, legacy)
}

pub fn emission_with_lp_profit_share(
    basis: u64,
    gain: u64,
    before: i128,
    share_from: i128,
    legacy: &mut u64,
) -> Result<u64, ProgramError> {
    let base = share_from.max(before).max(0);
    emission_with_allocation(
        basis,
        gain,
        before,
        Some((base, before.saturating_sub(base))),
        legacy,
    )
}

/// `origin` is recognized profit at zero new gain. A negative value means
/// equity first recovers an earlier peak or exposure reserve. Positive values
/// release previously funded profit before integrating this settlement's gain.
/// The legacy tail field remains untouched for funded-state compatibility.
#[inline(never)]
pub fn emission_with_allocation(
    basis: u64,
    gain: u64,
    before: i128,
    allocation: Option<(i128, i128)>,
    _legacy: &mut u64,
) -> Result<u64, ProgramError> {
    if basis == 0 {
        return Ok(0);
    }
    if gain == 0 {
        let denominator = before.max(FLAT_THRESHOLD) as u128;
        return u64_checked(
            Wide::product(basis as u128, 100 * FLAT_THRESHOLD as u128)
                .divide(Wide::from_u128(denominator))?
                .as_u128()?,
        );
    }
    let end = before
        .checked_add(gain as i128)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    if end <= FLAT_THRESHOLD {
        return u64_checked(basis as u128 * 100);
    }
    let average = if let Some((base, origin)) = allocation {
        if base < 0 || before.saturating_sub(base).saturating_sub(origin) < 0 {
            return Err(ProgramError::InvalidArgument);
        }
        let prefix = origin.saturating_neg().max(0).min(gain as i128) as u64;
        let remaining = gain - prefix;
        if remaining == 0 {
            plain_integral(before, gain)? / gain as u128
        } else {
            let recognized = origin.max(0) as u128;
            let total = u64_checked(recognized + remaining as u128)?;
            let start = base as u128 + recognized
                - lp_profit_share(base, u64_checked(recognized)?)? as u128;
            let end = base as u128 + total as u128 - lp_profit_share(base, total)? as u128;
            let offset = (before - base - origin) as u128;
            let shared = shared_average(start, end, offset)?;
            let early = if prefix > 0 {
                plain_integral(before, prefix)?
            } else {
                0
            };
            (remaining as u128)
                .checked_mul(shared)
                .and_then(|v| v.checked_add(early))
                .ok_or(ProgramError::ArithmeticOverflow)?
                / gain as u128
        }
    } else {
        plain_integral(before, gain)? / gain as u128
    };
    u64_checked(
        Wide::product(basis as u128, average)
            .divide(Wide::from_u128(LP_LOG_SCALE))?
            .as_u128()?,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quadratic_share_anchors_and_cash_domain() {
        for (dollars, bps) in [
            (0, 0),
            (4999, 0),
            (5000, 800),
            (100000, 832),
            (250000, 1000),
            (500000, 1600),
            (750000, 2600),
            (1000000, 4000),
            (5000000, 4000),
        ] {
            assert_eq!(lp_staker_share_bps(dollars * USDC as i128).unwrap(), bps);
        }
        assert_eq!(lp_staker_share_bps(u64::MAX as i128).unwrap(), 4000);
        assert!(lp_staker_share_bps(u64::MAX as i128 + 1).is_err());
        assert_eq!(lp_profit_share(i128::MIN, u64::MAX).unwrap(), 0);
        let got = lp_profit_share(LP_CURVE_END, u64::MAX).unwrap();
        assert_eq!(u64::MAX - got, ((u64::MAX as u128 * 3 + 4) / 5) as u64);
    }
    fn retained_reference(before: f64, profit: f64) -> f64 {
        let bootstrap = (5000.0 * USDC as f64 - before).max(0.0).min(profit);
        let mut left = profit - bootstrap;
        let mut at = before + bootstrap;
        let initial = at;
        if at < 1e12 && left > 0.0 {
            let q = (8.0f64 / 23.0).sqrt();
            let cost = 1e12 / (0.92 * q) * (q.atanh() - (q * at / 1e12).atanh());
            if left >= cost {
                at = 1e12;
                left -= cost;
            } else {
                at = 1e12 / q * ((q * at / 1e12).atanh() + 0.92 * q * left / 1e12).tanh();
                left = 0.0;
            }
        }
        bootstrap + at - initial + left * 0.6
    }
    #[test]
    fn quadratic_integral_matches_independent_hyperbolic_reference() {
        for before in [
            -1000, 0, 4999, 5000, 30000, 100000, 250000, 500000, 750000, 999999, 1000000, 5000000,
        ] {
            for profit in [1, 100, 10000, 1000000, 100000000] {
                let b = before * USDC as i128;
                let p = profit * USDC;
                let retained = p - lp_profit_share(b, p).unwrap();
                let reference = retained_reference(b as f64, p as f64);
                assert!(
                    (retained as f64 - reference).abs() < 5.0,
                    "before={before},profit={profit}: {retained} vs {reference}"
                );
                assert!(
                    retained as f64 + 0.01 >= reference,
                    "rounding must retain cash"
                );
            }
        }
    }
    #[test]
    fn allocation_splits_change_only_atomic_rounding() {
        for before in [
            0, 1000, 5000, 100000, 500000, 750000, 999999, 1000000, 5000000,
        ] {
            let initial = before * USDC as i128;
            let profit = 6_000_000 * USDC;
            let whole = profit - lp_profit_share(initial, profit).unwrap();
            let mut split = 0;
            for _ in 0..100 {
                let p = profit / 100;
                split += p - lp_profit_share(initial + split as i128, p).unwrap();
            }
            assert!(
                whole.abs_diff(split) < 300,
                "before={before},whole={whole},split={split}"
            );
        }
    }
    #[test]
    fn emission_follows_lp_drawdowns_and_ignores_legacy_tail() {
        for dollars in [500000u64, 1000000, 2000000, 5000000] {
            let mut legacy = u64::MAX;
            let got = emission(USDC, 0, dollars as i128 * USDC as i128, &mut legacy).unwrap();
            assert_eq!(got, 100 * USDC * 500000 / dollars);
            assert_eq!(legacy, u64::MAX);
        }
        for before in [i128::MIN, -1, 0, FLAT_THRESHOLD - 1, FLAT_THRESHOLD] {
            let mut legacy = 123;
            assert_eq!(emission(USDC, 0, before, &mut legacy).unwrap(), 100 * USDC);
            assert_eq!(legacy, 123);
        }
        let mut legacy = 123;
        let above = emission(1_000_000_000, 0, FLAT_THRESHOLD + 1, &mut legacy).unwrap();
        assert_eq!(above, 99_999_999_999);
        assert!(emission(u64::MAX, 0, 0, &mut legacy).is_err());
        assert_eq!(legacy, 123);
    }
    #[test]
    fn hyperbolic_emission_matches_logarithmic_reference_and_event_splits() {
        for before in [-1000, 0, 499999, 500000, 1000000, 5000000] {
            for dollars in [1u64, 10000, 1000000, 10000000] {
                let b = before * USDC as i128;
                let gain = dollars * USDC;
                let mut legacy = 55;
                let got = emission(gain, gain, b, &mut legacy).unwrap();
                let flat = (FLAT_THRESHOLD - b).max(0).min(gain as i128) as f64;
                let start = (b as f64 + flat).max(FLAT_THRESHOLD as f64);
                let reference = flat * 100.0 + 50e12 * ((gain as f64 - flat) / start).ln_1p();
                assert!(
                    (got as f64 - reference).abs() < 2.0,
                    "before={before},gain={gain}"
                );
                let mut sum = 0;
                for i in 0..100 {
                    sum += emission(
                        gain / 100,
                        gain / 100,
                        b + (gain / 100 * i) as i128,
                        &mut legacy,
                    )
                    .unwrap();
                }
                assert!(sum.abs_diff(got) <= 110);
                assert_eq!(legacy, 55);
            }
        }
    }
    #[test]
    fn shared_emission_crossings_and_reserve_offsets_are_batch_stable() {
        for before in [0, 499999, 500000, 750000, 999999, 1000000, 5000000] {
            for reserve in [0, 100000, 500000, 2000000] {
                let initial = before * USDC as i128;
                let base = (initial - reserve * USDC as i128).max(0);
                let origin = initial - reserve * USDC as i128 - base;
                let gain = 10_000_000 * USDC;
                let mut legacy = 999;
                let whole = emission_with_allocation(
                    gain,
                    gain,
                    initial,
                    Some((base, origin)),
                    &mut legacy,
                )
                .unwrap();
                let mut split = 0;
                let mut tracked = initial;
                let mut mark = base;
                for _ in 0..100 {
                    let p = gain / 100;
                    let origin = tracked - reserve * USDC as i128 - mark;
                    split +=
                        emission_with_allocation(p, p, tracked, Some((mark, origin)), &mut legacy)
                            .unwrap();
                    let share = lp_profit_share(mark, (origin + p as i128).max(0) as u64).unwrap();
                    tracked += p as i128 - share as i128;
                    mark = mark.max(tracked - reserve * USDC as i128);
                }
                assert!(
                    whole.abs_diff(split) < 100_000,
                    "before={before},reserve={reserve},whole={whole},split={split}"
                );
                assert!(whole <= gain * 100);
                assert_eq!(legacy, 999);
            }
        }
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
    // Independent adaptive Simpson integration uses floats in native tests only.
    fn simpson(f: &impl Fn(f64) -> f64, a: f64, b: f64, eps: f64, depth: u8) -> f64 {
        let m = (a + b) / 2.0;
        let l = (a + m) / 2.0;
        let r = (m + b) / 2.0;
        let whole = (b - a) / 6.0 * (f(a) + 4.0 * f(m) + f(b));
        let split =
            (m - a) / 6.0 * (f(a) + 4.0 * f(l) + f(m)) + (b - m) / 6.0 * (f(m) + 4.0 * f(r) + f(b));
        if depth == 0 || (split - whole).abs() < 15.0 * eps {
            split + (split - whole) / 15.0
        } else {
            simpson(f, a, m, eps / 2.0, depth - 1) + simpson(f, m, b, eps / 2.0, depth - 1)
        }
    }
    #[test]
    fn mint_path_quadrature_matches_independent_adaptive_integral() {
        for first in [
            0u64, 5000, 100000, 499999, 500000, 750000, 999999, 1000000, 5000000,
        ] {
            for width in [1u64, 100, 500000, 10000000] {
                for offset in [0u64, 100000, 500000, 1000000, 10000000] {
                    let last = first + width;
                    let got = shared_average(
                        first as u128 * USDC as u128,
                        last as u128 * USDC as u128,
                        offset as u128 * USDC as u128,
                    )
                    .unwrap() as f64
                        / LP_LOG_SCALE as f64;
                    let mut bounds = vec![first as f64 / 1e6, last as f64 / 1e6];
                    for cut in [0.005, 1.0, 0.5 - offset as f64 / 1e6] {
                        if cut > bounds[0] && cut < bounds[1] {
                            bounds.push(cut);
                        }
                    }
                    bounds.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    let mut cost = 0.0;
                    let mut reward = 0.0;
                    for pair in bounds.windows(2) {
                        let (a, b) = (pair[0], pair[1]);
                        let unshared = b <= 0.005;
                        let inv = |x: f64| {
                            if unshared {
                                1.0
                            } else {
                                1.0 / (0.92 - 0.32 * x.clamp(0.0, 1.0).powi(2))
                            }
                        };
                        let rate = |x: f64| (50.0 / (x + offset as f64 / 1e6)).min(100.0);
                        cost += simpson(&inv, a, b, 1e-14, 24);
                        reward += simpson(&|x| rate(x) * inv(x), a, b, 1e-14, 24);
                    }
                    let expected = reward / cost;
                    assert!(
                        (got - expected).abs() * cost * 1e12 < 5.0,
                        "first={first},width={width},offset={offset}: {got} vs {expected}"
                    );
                }
            }
        }
    }
}

#[cfg(kani)]
#[path = "../../formal-verification/harnesses/wide.rs"]
mod verification;
