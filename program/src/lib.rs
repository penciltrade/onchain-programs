#![allow(unexpected_cfgs)]
#[cfg(test)]
mod hardening_tests;
pub mod ledger;
pub mod math;
#[cfg(test)]
mod recovery_tests;
pub mod storage;
pub mod types;
use borsh::BorshDeserialize;
use ledger::*;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    bpf_loader_upgradeable,
    clock::Clock,
    entrypoint::ProgramResult,
    instruction::{AccountMeta, Instruction as SolIx},
    program::{get_return_data, invoke, invoke_signed, set_return_data},
    program_error::ProgramError,
    program_pack::Pack,
    pubkey::Pubkey,
    rent::Rent,
    system_instruction,
    sysvar::Sysvar,
};
use spl_token::state::{Account as TokenAccount, Mint};
use types::*;
#[cfg(not(feature = "no-entrypoint"))]
solana_program::entrypoint!(process_instruction);
// All SDK transactions request a 256KiB heap frame. The default Solana
// allocator remains 32KiB even when the runtime frame is increased.
#[cfg(all(target_os = "solana", feature = "custom-heap"))]
#[global_allocator]
static ALLOC: solana_program::entrypoint::BumpAllocator =
    solana_program::entrypoint::BumpAllocator {
        start: solana_program::entrypoint::HEAP_START_ADDRESS as usize,
        len: 256 * 1024,
    };
const BAD_ORACLE: u32 = 100;
const BAD_SEQUENCE: u32 = 101;
/// How far ahead of the owner's next nonce an admission may run.
const MAX_NONCE_GAP: u64 = 1024;
/// How far behind it an admission may land. An owner's orders signed in
/// parallel can land in any order; each nonce still keys exactly one live
/// intent record, and a signed transaction cannot land twice.
const NONCE_WINDOW: u64 = 64;
const PAUSED: u32 = 102;
const IMMUTABLE: u32 = 104;
const NOT_SEALED: u32 = 105;
const RECOVERY_DELAY: u32 = 106;
/// Internal only: the oracle shows a different mark for a slot this program has
/// already consumed. It clears when the mark slot advances, so the queue waits.
/// Every instruction surfaces it to callers as BAD_ORACLE.
const MARK_CONFLICT: u32 = 107;
fn invalid() -> ProgramError {
    ProgramError::InvalidArgument
}
fn signer(a: &AccountInfo) -> ProgramResult {
    if !a.is_signer {
        Err(ProgramError::MissingRequiredSignature)
    } else {
        Ok(())
    }
}
fn account<'a, 'b>(
    a: &'a [AccountInfo<'b>],
    i: usize,
) -> Result<&'a AccountInfo<'b>, ProgramError> {
    a.get(i).ok_or(ProgramError::NotEnoughAccountKeys)
}
fn unpack_token(a: &AccountInfo) -> Result<TokenAccount, ProgramError> {
    if a.owner != &spl_token::id() {
        return Err(ProgramError::IncorrectProgramId);
    }
    TokenAccount::unpack(&a.try_borrow_data()?)
}
fn check_user_token(a: &AccountInfo, owner: &Pubkey, mint: &Pubkey) -> ProgramResult {
    let t = unpack_token(a)?;
    if &t.owner != owner || &t.mint != mint {
        return Err(invalid());
    }
    Ok(())
}
pub fn read_state(a: &AccountInfo) -> Result<State, ProgramError> {
    let d = a.try_borrow_data()?;
    if d.len() < 4 {
        return Err(ProgramError::UninitializedAccount);
    }
    let n = u32::from_le_bytes(d[..4].try_into().unwrap()) as usize;
    if n == 0 || n > d.len() - 4 {
        return Err(ProgramError::UninitializedAccount);
    }
    let s = State::try_from_slice(&d[4..4 + n]).map_err(|_| ProgramError::InvalidAccountData)?;
    if s.version != 4 {
        return Err(ProgramError::InvalidAccountData);
    }
    Ok(s)
}
fn save_state(a: &AccountInfo, s: &State) -> ProgramResult {
    let bytes = borsh::to_vec(s).map_err(|_| ProgramError::InvalidAccountData)?;
    let needed = bytes.len() + 4;
    if needed > STATE_SIZE {
        return Err(ProgramError::AccountDataTooSmall);
    }
    if needed > a.data_len() {
        let size = ((needed + 4095) / 4096) * 4096;
        if size - a.data_len() > 10240 {
            return Err(ProgramError::AccountDataTooSmall);
        }
        a.realloc(size, false)?;
    }
    let mut d = a.try_borrow_mut_data()?;
    d[..4].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
    d[4..4 + bytes.len()].copy_from_slice(&bytes);
    Ok(())
}
fn validate_common(id: &Pubkey, a: &[AccountInfo], s: &State) -> ProgramResult {
    let state = account(a, 0)?;
    let (expected, bump) = Pubkey::find_program_address(&[b"state"], id);
    if state.key != &expected || state.owner != id || bump != s.bump || !state.is_writable {
        return Err(ProgramError::InvalidAccountData);
    }
    signer(account(a, 1)?)?;
    if account(a, 2)?.key != &s.usdc_vault
        || account(a, 3)?.key != &s.pencil_mint
        || account(a, 4)?.key != &s.stake_vault
        || account(a, 5)?.key != &spl_token::id()
        || !account(a, 5)?.executable
    {
        return Err(ProgramError::IncorrectProgramId);
    }
    let v = unpack_token(account(a, 2)?)?;
    let st = unpack_token(account(a, 4)?)?;
    if v.owner != *state.key
        || v.mint != s.usdc_mint
        || st.owner != *state.key
        || st.mint != s.pencil_mint
    {
        return Err(ProgramError::InvalidAccountData);
    }
    let m = Mint::unpack(&account(a, 3)?.try_borrow_data()?)?;
    if account(a, 3)?.owner != &spl_token::id()
        || m.decimals != 6
        || m.mint_authority != solana_program::program_option::COption::Some(*state.key)
        || m.freeze_authority.is_some()
    {
        return Err(ProgramError::InvalidAccountData);
    }
    // Unsolicited transfers are uncredited excess, never a house deposit or liability.
    if v.amount < expected_vault(s)? || st.amount < add(s.total_stake, s.total_pending_stake)? {
        return Err(ProgramError::InvalidAccountData);
    }
    Ok(())
}
fn token_transfer(
    a: &[AccountInfo],
    from: usize,
    to: usize,
    authority: usize,
    amount: u64,
    signed: bool,
    bump: u8,
) -> ProgramResult {
    let ix = spl_token::instruction::transfer(
        &spl_token::id(),
        account(a, from)?.key,
        account(a, to)?.key,
        account(a, authority)?.key,
        &[],
        amount,
    )?;
    let infos = [
        account(a, from)?.clone(),
        account(a, to)?.clone(),
        account(a, authority)?.clone(),
        account(a, 5)?.clone(),
    ];
    if signed {
        invoke_signed(&ix, &infos, &[&[b"state", &[bump]]])
    } else {
        invoke(&ix, &infos)
    }
}
fn initialize(
    id: &Pubkey,
    a: &[AccountInfo],
    local_mode: bool,
    devnet: bool,
    sequencer: Pubkey,
    oracle_program: Pubkey,
    guardian: Pubkey,
    team: Pubkey,
    admins: [Pubkey; 3],
    governance_delay: i64,
    markets: Vec<Market>,
) -> ProgramResult {
    let mut it = a.iter();
    let payer = next_account_info(&mut it)?;
    let state = next_account_info(&mut it)?;
    let usdc = next_account_info(&mut it)?;
    let pencil = next_account_info(&mut it)?;
    let vault = next_account_info(&mut it)?;
    let stake = next_account_info(&mut it)?;
    let token = next_account_info(&mut it)?;
    let system = next_account_info(&mut it)?;
    let program = next_account_info(&mut it)?;
    let programdata = next_account_info(&mut it)?;
    signer(payer)?;
    let (expected, bump) = Pubkey::find_program_address(&[b"state"], id);
    if state.key != &expected
        || state.owner != &solana_program::system_program::id()
        || state.data_len() != 0
    {
        return Err(ProgramError::AccountAlreadyInitialized);
    }
    if token.key != &spl_token::id() || system.key != &solana_program::system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
    }
    let pdkey = Pubkey::find_program_address(&[id.as_ref()], &bpf_loader_upgradeable::id()).0;
    if program.key != id
        || !program.executable
        || program.owner != &bpf_loader_upgradeable::id()
        || programdata.key != &pdkey
        || programdata.owner != &bpf_loader_upgradeable::id()
    {
        return Err(ProgramError::IncorrectProgramId);
    }
    if loader_authority(id, program, programdata)? != Some(*payer.key) {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if admins[0] == admins[1] || admins[1] == admins[2] || admins[0] == admins[2] {
        return Err(invalid());
    }
    validate_config(
        local_mode,
        devnet,
        &oracle_program,
        governance_delay,
        &markets,
    )?;
    let um = Mint::unpack(&usdc.try_borrow_data()?)?;
    let pm = Mint::unpack(&pencil.try_borrow_data()?)?;
    if (!local_mode && !devnet && usdc.key != &MAINNET_USDC)
        || usdc.owner != &spl_token::id()
        || pencil.owner != &spl_token::id()
        || um.decimals != 6
        || pm.decimals != 6
        || pm.supply != 0
        || pm.mint_authority != solana_program::program_option::COption::Some(expected)
        || pm.freeze_authority.is_some()
    {
        return Err(invalid());
    }
    let vt = unpack_token(vault)?;
    let st = unpack_token(stake)?;
    if vt.owner != expected
        || st.owner != expected
        || vt.mint != *usdc.key
        || st.mint != *pencil.key
    {
        return Err(invalid());
    }
    if !vault_is_exclusive(&vt) || !vault_is_exclusive(&st) {
        return Err(invalid());
    }
    // Accept prefunding/dust without allowing it to select protocol configuration.
    // CPI allocation/growth is bounded to 10KiB; prefund the maximum rent.
    let minimum = Rent::get()?.minimum_balance(STATE_SIZE);
    if state.lamports() < minimum {
        invoke(
            &system_instruction::transfer(payer.key, state.key, minimum - state.lamports()),
            &[payer.clone(), state.clone(), system.clone()],
        )?;
    }
    invoke_signed(
        &system_instruction::allocate(state.key, 8192),
        &[state.clone(), system.clone()],
        &[&[b"state", &[bump]]],
    )?;
    invoke_signed(
        &system_instruction::assign(state.key, id),
        &[state.clone(), system.clone()],
        &[&[b"state", &[bump]]],
    )?;
    let s = State {
        version: 4,
        bump,
        local_mode,
        devnet,
        usdc_mint: *usdc.key,
        pencil_mint: *pencil.key,
        usdc_vault: *vault.key,
        stake_vault: *stake.key,
        sequencer,
        oracle_program,
        guardian,
        team,
        admins,
        paused: false,
        immutable: false,
        permissionless: false,
        started_at: 0,
        last_sequenced_at: 0,
        max_leverage: 200,
        governance_delay,
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
        stake_warmup: if local_mode {
            8
        } else if devnet {
            DEVNET_STAKE_WARMUP
        } else {
            STAKE_WARMUP
        },
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
        markets,
        users: vec![],
        positions: vec![],
        claims: vec![],
        intents: vec![],
        outcomes: vec![],
        proposals: vec![],
    };
    save_state(state, &s)
}
/// One guarded Hawkeye observation. Execution and busts use the Phoenix mark
/// (a median of oracle-adjusted spot, Phoenix book and external-exchange perps);
/// the touch is not a price input, so improving or sweeping Phoenix's book
/// alone cannot set a fill or force a bust.
#[derive(Clone, Copy)]
struct Quote {
    mark: u64,
    mark_slot: u64,
    index_slot: u64,
}
impl Quote {
    fn busted(self, p: &Position) -> bool {
        math::busted(p.entry, self.mark, p.side, p.leverage)
    }
}
pub const PHOENIX_PROGRAM: &str = "EtrnLzgbS7nMMy5fbD42kXiUzGg8XQzJ972Xtk1cjWih";
pub const HAWKEYE_PROGRAM: &str = "RiSeVw3ZjNfsaXPRb4mgaqYaEEt41pNNJoDvVh7pgQj";
pub const MAINNET_USDC: Pubkey =
    solana_program::pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
/// Deployment-mode rules for the oracle, governance delay and fill timing.
/// Mainnet must call the real Hawkeye view over Phoenix-owned accounts; devnet
/// must call a stub whose own accounts carry the quotes; local is unrestricted.
pub fn validate_config(
    local_mode: bool,
    devnet: bool,
    oracle_program: &Pubkey,
    governance_delay: i64,
    markets: &[Market],
) -> ProgramResult {
    let real: Pubkey = HAWKEYE_PROGRAM.parse().map_err(|_| invalid())?;
    let phoenix: Pubkey = PHOENIX_PROGRAM.parse().map_err(|_| invalid())?;
    let mainnet = !local_mode && !devnet;
    if (local_mode && devnet)
        || governance_delay < min_governance_delay(local_mode, devnet)
        || markets.is_empty()
        || markets.len() > 8
        || (mainnet != (*oracle_program == real))
    {
        return Err(invalid());
    }
    for (i, m) in markets.iter().enumerate() {
        if m.id as usize != i
            || m.oracle_accounts.is_empty()
            || m.oracle_accounts.len() > 32
            || m.tick_price_micro == 0
            || m.position_multiplier_milli == 0
            || m.position_multiplier_milli > MAX_POSITION_MULTIPLIER_MILLI
            || m.next_seq != 1
            || m.last_mark_slot != 0
            || m.last_mark != 0
            || m.last_mark_at != 0
            || m.halted
            || m.long_units != 0
            || m.short_notional != 0
            || m.quote_read_slot != 0
            || m.quote_version != 0
            || m.quote_mark != 0
            || m.quote_mark_slot != 0
            || m.quote_index_slot != 0
            || m.quote_status != 0
            || m.max_age_slots == 0
            || m.max_spread_bps > 1000
            || m.max_deviation_bps > 1000
        {
            return Err(invalid());
        }
        validate_fill(local_mode, m.fill_delay_slots, m.fill_window_slots)?;
        // One delay for every market: the shared FIFO then never holds an
        // order behind a head whose fill window opens later than its own.
        if m.fill_delay_slots != markets[0].fill_delay_slots {
            return Err(invalid());
        }
        if mainnet && (m.oracle_accounts.len() < 7 || m.oracle_accounts[0] != phoenix) {
            return Err(invalid());
        }
    }
    Ok(())
}
/// Handing a token account to the state PDA does not clear a close authority
/// or a delegate. A close authority could close an empty vault, after which
/// every instruction would fail; a delegate could move funds out of it.
pub fn vault_is_exclusive(t: &TokenAccount) -> bool {
    t.delegate.is_none() && t.close_authority.is_none()
}
/// Per-position size limit, in USDC atoms of margin times leverage.
pub fn notional_within_cap(margin: u64, leverage: u16) -> bool {
    margin as u128 * leverage as u128 <= MAX_NOTIONAL
}
/// Outside local fixtures, a fill must use a mark from a later slot than the
/// admission, so neither the trader nor the drainer chooses the fill price.
pub fn validate_fill(local_mode: bool, delay: u16, window: u16) -> ProgramResult {
    if delay > MAX_FILL_DELAY_SLOTS
        || window == 0
        || window > MAX_FILL_WINDOW_SLOTS
        || (!local_mode && delay == 0)
    {
        return Err(invalid());
    }
    Ok(())
}
/// Whether `price` is an acceptable successor to the last consumed mark.
pub fn mark_within_band(last_mark: u64, price: u128) -> bool {
    last_mark == 0
        || price.abs_diff(last_mark as u128) * 10_000 <= last_mark as u128 * MAX_MARK_MOVE_BPS
}
/// A guarded observation for every path except the queue head, which treats a
/// same-slot conflict as "wait" rather than as a rejection.
fn oracle(s: &mut State, a: &[AccountInfo], market: u16) -> Result<Quote, ProgramError> {
    read_oracle(s, a, market).map_err(|e| match e {
        ProgramError::Custom(MARK_CONFLICT) => ProgramError::Custom(BAD_ORACLE),
        e => e,
    })
}
fn read_oracle(s: &mut State, a: &[AccountInfo], market: u16) -> Result<Quote, ProgramError> {
    let slot = Clock::get()?.slot;
    let m = s.markets.get(market as usize).ok_or_else(invalid)?;
    if m.halted {
        return Err(ProgramError::Custom(PAUSED));
    }
    if m.quote_status != 0 && m.quote_read_slot == slot {
        // A configuration change may invalidate this slot's quote, but cannot
        // cause a second read in that slot. The next slot reads the new view.
        if m.quote_version != m.version {
            return Err(ProgramError::Custom(BAD_ORACLE));
        }
        return match m.quote_status {
            1 => Ok(Quote {
                mark: m.quote_mark,
                mark_slot: m.quote_mark_slot,
                index_slot: m.quote_index_slot,
            }),
            3 => Err(ProgramError::Custom(MARK_CONFLICT)),
            _ => Err(ProgramError::Custom(BAD_ORACLE)),
        };
    }
    let result = read_oracle_uncached(s, a, market);
    let status = match &result {
        Ok(_) => 1,
        Err(ProgramError::Custom(BAD_ORACLE)) => 2,
        Err(ProgramError::Custom(MARK_CONFLICT)) => 3,
        _ => return result,
    };
    let m = &mut s.markets[market as usize];
    m.quote_read_slot = slot;
    m.quote_version = m.version;
    m.quote_status = status;
    if let Ok(q) = &result {
        m.quote_mark = q.mark;
        m.quote_mark_slot = q.mark_slot;
        m.quote_index_slot = q.index_slot;
    } else {
        m.quote_mark = 0;
        m.quote_mark_slot = 0;
        m.quote_index_slot = 0;
    }
    result
}
fn read_oracle_uncached(s: &State, a: &[AccountInfo], market: u16) -> Result<Quote, ProgramError> {
    let m = s.markets.get(market as usize).ok_or_else(invalid)?;
    if m.halted {
        return Err(ProgramError::Custom(PAUSED));
    }
    let program = account(a, 9)?;
    if program.key != &s.oracle_program || !program.executable {
        return Err(ProgramError::IncorrectProgramId);
    }
    let mut infos = Vec::with_capacity(m.oracle_accounts.len() + 1);
    let mut metas = Vec::with_capacity(m.oracle_accounts.len());
    for (index, key) in m.oracle_accounts.iter().enumerate() {
        let ac = a
            .get(12..)
            .ok_or(ProgramError::NotEnoughAccountKeys)?
            .iter()
            .find(|x| x.key == key)
            .ok_or(ProgramError::NotEnoughAccountKeys)?;
        let wrong_owner = if s.devnet {
            ac.owner != &s.oracle_program
        } else if !s.local_mode {
            (index == 0 && !ac.executable) || (index > 0 && ac.owner != &m.oracle_accounts[0])
        } else {
            false
        };
        if wrong_owner {
            // A pinned account that changed hands is an oracle fault, handled
            // like any other rejected quote instead of failing the transaction.
            return Err(ProgramError::Custom(BAD_ORACLE));
        }
        metas.push(AccountMeta::new_readonly(*key, false));
        infos.push(ac.clone());
    }
    // Every mode speaks Hawkeye's view_bbo ABI; local and devnet stubs answer it.
    let data = solana_program::hash::hash(b"global:view_bbo").to_bytes()[..8].to_vec();
    infos.push(program.clone());
    invoke(
        &SolIx {
            program_id: s.oracle_program,
            accounts: metas,
            data,
        },
        &infos,
    )?;
    let (owner, ret) = get_return_data().ok_or(ProgramError::Custom(BAD_ORACLE))?;
    if owner != s.oracle_program
        || ret.len() != 64
        || ret[..8] != [0x71, 0x41, 0xa7, 0x1f, 0xa3, 0x1f, 0xca, 0xef]
        || ret[8..10] != 1u16.to_le_bytes()
    {
        return Err(ProgramError::Custom(BAD_ORACLE));
    }
    let r = |off: usize| u64::from_le_bytes(ret[off..off + 8].try_into().unwrap());
    let (mark, index, mark_slot, index_slot) = (r(32), r(40), r(48), r(56));
    let now = Clock::get()?.slot;
    // A mark too large to price is a rejected quote, not a failed transaction.
    let price = mark as u128 * m.tick_price_micro as u128;
    if price > u64::MAX as u128 {
        return Err(ProgramError::Custom(BAD_ORACLE));
    }
    // Circuit breakers only: a fresh mark and index that agree within the band.
    // A one-sided, crossed or wide Phoenix book no longer blocks execution.
    if mark == 0
        || index == 0
        || mark_slot > now
        || index_slot > now
        || now - mark_slot > m.max_age_slots
        || now - index_slot > m.max_age_slots
        || mark_slot < m.last_mark_slot
        || (mark as u128).abs_diff(index as u128) * 10_000
            > index as u128 * m.max_deviation_bps as u128
    {
        return Err(ProgramError::Custom(BAD_ORACLE));
    }
    if mark_slot == m.last_mark_slot && m.last_mark != 0 && price != m.last_mark as u128 {
        return Err(ProgramError::Custom(MARK_CONFLICT));
    }
    // Mark and index come from the same upstream program, so a change of scale
    // there moves both together and passes the deviation check above. Bound
    // the mark against the last one this program accepted. Local fixtures step
    // prices freely.
    if !s.local_mode && !mark_within_band(m.last_mark, price) {
        return Err(ProgramError::Custom(BAD_ORACLE));
    }
    Ok(Quote {
        mark: math::u64_checked(price)?,
        mark_slot,
        index_slot,
    })
}
/// Admission needs a recent checkpoint for the market. Anyone can publish one
/// (CheckpointOracle, a fill or a liquidation), so a healthy market stays
/// open, while a market whose oracle no longer answers stops receiving
/// intents that would hold the shared FIFO head until they expire.
fn admissible(s: &State, market: u16, now: i64) -> ProgramResult {
    let m = s.markets.get(market as usize).ok_or_else(invalid)?;
    if m.halted {
        return Err(ProgramError::Custom(PAUSED));
    }
    if !s.local_mode
        && (m.last_mark == 0 || now.saturating_sub(m.last_mark_at) > ADMISSION_MARK_MAX_AGE)
    {
        return Err(ProgramError::Custom(BAD_ORACLE));
    }
    Ok(())
}
/// Record the newest mark consumed so a later instruction can never price
/// against an older one, even if an oracle account could present it.
fn consume_mark(s: &mut State, market: u16, q: &Quote) -> ProgramResult {
    checkpoint(s, market, q.mark, q.mark_slot, Clock::get()?.unix_timestamp)
}
/// Only advancing, nonconflicting observations renew oracle liveness.
pub fn checkpoint(s: &mut State, market: u16, price: u64, slot: u64, now: i64) -> ProgramResult {
    let m = s.markets.get_mut(market as usize).ok_or_else(invalid)?;
    if m.halted
        || price == 0
        || slot < m.last_mark_slot
        || (slot == m.last_mark_slot && m.last_mark != 0 && price != m.last_mark)
    {
        return Err(ProgramError::Custom(BAD_ORACLE));
    }
    if m.last_mark == 0 || slot > m.last_mark_slot {
        m.last_mark_slot = slot;
        m.last_mark = price;
        m.last_mark_at = now;
    }
    Ok(())
}
/// Explicit, one-way removal after 24 hours without admission or heartbeat.
/// No caller, including the guardian, can bypass the inactivity threshold.
pub fn remove_relayer(s: &mut State, now: i64) -> ProgramResult {
    if !s.immutable {
        return Err(ProgramError::Custom(NOT_SEALED));
    }
    if !s.permissionless && now.saturating_sub(s.last_sequenced_at) < SEQUENCER_TIMEOUT
    {
        return Err(ProgramError::Custom(RECOVERY_DELAY));
    }
    s.permissionless = true;
    Ok(())
}
/// This proves missing protocol checkpoints, not upstream oracle failure.
/// No external CPI runs on the recovery path, even if that program is broken.
pub fn wind_down_market(s: &mut State, market: u16, now: i64, slot: u64) -> ProgramResult {
    let m = s.markets.get_mut(market as usize).ok_or_else(invalid)?;
    if m.halted {
        return Ok(());
    }
    let last = if m.last_mark == 0 {
        s.started_at
    } else {
        m.last_mark_at
    };
    if now.saturating_sub(last) < ORACLE_TIMEOUT
        || (m.last_mark != 0 && slot.saturating_sub(m.last_mark_slot) < ORACLE_MIN_SLOTS)
    {
        return Err(ProgramError::Custom(RECOVERY_DELAY));
    }
    m.halted = true;
    if s.markets.iter().all(|m| m.halted) {
        s.paused = true;
    }
    Ok(())
}
pub fn settle_wind_down(s: &mut State, position_id: u64) -> Result<Outcome, ProgramError> {
    let p = s
        .positions
        .iter()
        .position(|p| p.id == position_id)
        .ok_or_else(invalid)?;
    let position = s.positions[p].clone();
    let m = s
        .markets
        .get(position.market as usize)
        .ok_or_else(invalid)?;
    if !m.halted || m.last_mark == 0 {
        return Err(invalid());
    }
    let price = m.last_mark;
    let bust = math::busted(position.entry, price, position.side, position.leverage);
    // This retires no queue ticket. Existing close intents become invalid and
    // are released later by FIFO drain, including after position rent reclaim.
    let priority = s.next_ticket;
    settle_position(s, p, price, priority, bust)
}
pub fn cancel_wind_down_intent(s: &mut State, ticket: u64) -> ProgramResult {
    let n = s
        .intents
        .iter()
        .position(|i| i.ticket == ticket)
        .ok_or_else(invalid)?;
    let i = s.intents[n].clone();
    if !s.markets.get(i.market as usize).ok_or_else(invalid)?.halted {
        return Err(invalid());
    }
    if i.kind == 2 {
        return Ok(());
    }
    restore_intent(s, &i)?;
    // Keep its record until FIFO consumes the tombstone. Deleting it here would
    // strand execute_ticket and block unrelated, still-live markets forever.
    s.intents[n].kind = 2;
    s.intents[n].margin = 0;
    s.intents[n].claim_id = 0;
    s.intents[n].position_id = 0;
    record(
        s,
        Outcome {
            ticket,
            code: 4,
            position_id: i.position_id,
            price: 0,
            pnl: 0,
            minted_credit: 0,
        },
    );
    Ok(())
}
/// Validate both loader accounts rather than trusting a caller-supplied header.
fn loader_authority(
    id: &Pubkey,
    program: &AccountInfo,
    pd: &AccountInfo,
) -> Result<Option<Pubkey>, ProgramError> {
    let expected = Pubkey::find_program_address(&[id.as_ref()], &bpf_loader_upgradeable::id()).0;
    if program.key != id
        || !program.executable
        || program.owner != &bpf_loader_upgradeable::id()
        || pd.key != &expected
        || pd.owner != &bpf_loader_upgradeable::id()
    {
        return Err(ProgramError::IncorrectProgramId);
    }
    let data = program.try_borrow_data()?;
    if data.len() != 36 || data[..4] != 2u32.to_le_bytes() || data[4..36] != expected.to_bytes() {
        return Err(ProgramError::InvalidAccountData);
    }
    drop(data);
    let data = pd.try_borrow_data()?;
    if data.len() < 45 || data[..4] != 3u32.to_le_bytes() {
        return Err(ProgramError::InvalidAccountData);
    }
    match data[12] {
        0 => Ok(None),
        1 => Ok(Some(Pubkey::new_from_array(
            data[13..45].try_into().unwrap(),
        ))),
        _ => Err(ProgramError::InvalidAccountData),
    }
}
fn seal(id: &Pubkey, a: &[AccountInfo], s: &mut State, now: i64) -> ProgramResult {
    if !s.immutable && (s.paused || s.markets.iter().any(|m| m.halted)) {
        return Err(ProgramError::Custom(PAUSED));
    }
    let program = account(a, 12)?;
    let pd = account(a, 13)?;
    let loader = account(a, 14)?;
    if loader.key != &bpf_loader_upgradeable::id() || !loader.executable || !pd.is_writable {
        return Err(ProgramError::IncorrectProgramId);
    }
    if let Some(authority) = loader_authority(id, program, pd)? {
        let signer = if authority == *account(a, 0)?.key {
            account(a, 0)?
        } else if authority == *account(a, 1)?.key {
            account(a, 1)?
        } else {
            return Err(ProgramError::MissingRequiredSignature);
        };
        let ix = bpf_loader_upgradeable::set_upgrade_authority(id, &authority, None);
        let infos = [pd.clone(), signer.clone(), loader.clone()];
        if signer.key == account(a, 0)?.key {
            invoke_signed(&ix, &infos, &[&[b"state", &[s.bump]]])?;
        } else {
            invoke(&ix, &infos)?;
        }
    }
    if !s.immutable {
        s.immutable = true;
        s.started_at = now;
        s.last_sequenced_at = now;
    }
    Ok(())
}
fn disabled_governance(ix: &Instruction) -> bool {
    matches!(
        ix,
        Instruction::Propose { .. }
            | Instruction::Approve { .. }
            | Instruction::Apply { .. }
            | Instruction::ProposeUpgrade { .. }
            | Instruction::ApplyUpgrade { .. }
            | Instruction::ProposeRotation { .. }
            | Instruction::ApplyRotation { .. }
            | Instruction::SetGovernanceDelay { .. }
    )
}

fn record(s: &mut State, o: Outcome) {
    // Settlements emit their position context before removing the position.
    if !matches!(o.code, 1 | 2) {
        emit_chain_event(&ChainEvent::Outcome {
            outcome: o.clone(),
            position: s.positions.iter().find(|p| p.id == o.position_id).cloned(),
        });
    }
    if s.outcomes.len() >= 32 {
        s.outcomes.remove(0);
    }
    s.outcomes.push(o);
}
pub fn emit_chain_event(event: &ChainEvent) {
    let bytes = borsh::to_vec(event).expect("Cannot encode chain event");
    solana_program::log::sol_log_data(&[b"PENCIL_EVENT_V1", &bytes]);
}
fn restore_intent(s: &mut State, i: &Intent) -> ProgramResult {
    if i.kind == 0 {
        let u = user_index(s, &i.owner)?;
        if i.claim_id == 0 {
            s.users[u].pending_cash = sub(s.users[u].pending_cash, i.margin)?;
            s.users[u].available = add(s.users[u].available, i.margin)?;
        } else {
            let c = s
                .claims
                .iter_mut()
                .find(|c| c.id == i.claim_id)
                .ok_or(ProgramError::NotEnoughAccountKeys)?;
            c.reserved = sub(c.reserved, i.margin)?;
            c.amount = add(c.amount, i.margin)?;
        }
        // An unfilled delegate open returns its budget, but only to the same
        // delegate session: a rotated or revoked epoch starts from a fresh budget.
        let x = &mut s.users[u];
        if i.signer != i.owner && x.delegate == i.signer && x.delegate_epoch == i.delegate_epoch {
            x.delegate_budget = add(x.delegate_budget, i.margin)?;
        }
    } else if let Some(p) = s.positions.iter_mut().find(|p| p.id == i.position_id) {
        p.close_pending = false;
    }
    Ok(())
}
fn drain(s: &mut State, a: &[AccountInfo], max: u16) -> ProgramResult {
    let now = Clock::get()?.unix_timestamp;
    // Phoenix accounts are readonly throughout this instruction. One current
    // observation per market prices all ordered actions in the same batch.
    let mut prices: [Option<Quote>; 8] = [None; 8];
    // A rejection is remembered the same way, so a run of intents on a market
    // that cannot be priced costs one oracle call, like a run on a healthy one.
    let mut rejected = [false; 8];
    for _ in 0..max.min(MAX_BATCH as u16) {
        if s.intents.is_empty() {
            break;
        }
        let i = s.intents[0].clone();
        if i.ticket != s.execute_ticket {
            break;
        } // supplied records must start at the global head
        if i.kind == 2 {
            s.intents.remove(0);
            s.execute_ticket = add(s.execute_ticket, 1)?;
            continue;
        }
        let Ok(u) = user_index(s, &i.owner) else {
            break;
        };
        // Only an open draws on a claim. Waiting here for a claim record is safe
        // for opens, whose claim stays live while they reserve it; a close must
        // never wait on one, or a bogus reference would block the queue for good.
        if i.kind == 0 && i.claim_id != 0 && !s.claims.iter().any(|c| c.id == i.claim_id) {
            break;
        }
        // A delegate's authority is checked when its intent is admitted and is
        // not looked at again here. Revoking or rotating the delegate stops
        // new admissions; it cannot withdraw an order already in the queue,
        // which would let the owner cancel after seeing the fill price.
        let position_key =
            storage::address(a[0].owner, storage::POSITION, &i.position_id.to_le_bytes())?.0;
        if i.kind == 1 && !a.iter().any(|a| a.key == &position_key) {
            break;
        }
        let invalid_close = i.kind == 1
            && !s
                .positions
                .iter()
                .any(|p| p.id == i.position_id && p.owner == i.owner);
        if i.expiry <= now
            || invalid_close
            || (i.kind == 0 && s.paused)
            || s.markets[i.market as usize].halted
        {
            restore_intent(s, &i)?;
            s.intents.remove(0);
            s.execute_ticket = add(s.execute_ticket, 1)?;
            record(
                s,
                Outcome {
                    ticket: i.ticket,
                    code: if i.expiry <= now { 3 } else { 4 },
                    position_id: i.position_id,
                    price: 0,
                    pnl: 0,
                    minted_credit: 0,
                },
            );
            continue;
        }
        let quote = if let Some(p) = prices[i.market as usize] {
            p
        } else {
            let read = if rejected[i.market as usize] {
                Err(ProgramError::Custom(BAD_ORACLE))
            } else {
                read_oracle(s, a, i.market)
            };
            match read {
                Ok(p) => {
                    prices[i.market as usize] = Some(p);
                    p
                }
                // Wait: the drainer left the oracle accounts out, or the mark
                // slot has to advance past a conflicting observation.
                Err(ProgramError::NotEnoughAccountKeys)
                | Err(ProgramError::Custom(MARK_CONFLICT)) => break,
                Err(ProgramError::Custom(BAD_ORACLE)) => {
                    rejected[i.market as usize] = true;
                    // The market cannot be priced. Before this intent's fill
                    // window opens it could not have filled anyway, so wait.
                    // After that, end it and restore its reservation instead of
                    // holding every other market behind it until it expires.
                    let delay = s.markets[i.market as usize].fill_delay_slots as u64;
                    if delay == 0 || Clock::get()?.slot < i.admitted_slot.saturating_add(delay) {
                        break;
                    }
                    restore_intent(s, &i)?;
                    s.intents.remove(0);
                    s.execute_ticket = add(s.execute_ticket, 1)?;
                    record(
                        s,
                        Outcome {
                            ticket: i.ticket,
                            code: 6,
                            position_id: i.position_id,
                            price: 0,
                            pnl: 0,
                            minted_credit: 0,
                        },
                    );
                    continue;
                }
                Err(e) => return Err(e),
            }
        };
        // Slot-anchored fill: only a mark updated in [admitted + delay,
        // admitted + delay + window] can price this intent. Earlier marks wait
        // (and hold FIFO); a window that passes unfilled ends the intent.
        let (delay, window) = {
            let m = &s.markets[i.market as usize];
            (m.fill_delay_slots as u64, m.fill_window_slots as u64)
        };
        if delay > 0 {
            let opens = i.admitted_slot.saturating_add(delay);
            if quote.mark_slot < opens {
                break;
            }
            if quote.mark_slot > opens.saturating_add(window) {
                restore_intent(s, &i)?;
                s.intents.remove(0);
                s.execute_ticket = add(s.execute_ticket, 1)?;
                record(
                    s,
                    Outcome {
                        ticket: i.ticket,
                        code: 6,
                        position_id: i.position_id,
                        price: 0,
                        pnl: 0,
                        minted_credit: 0,
                    },
                );
                continue;
            }
        }
        consume_mark(s, i.market, &quote)?;
        let price = quote.mark;
        let limit_failed = if i.kind == 0 {
            (i.side == 1 && price > i.limit_price) || (i.side == -1 && price < i.limit_price)
        } else {
            let p = s
                .positions
                .iter()
                .find(|p| p.id == i.position_id)
                .ok_or_else(invalid)?;
            !quote.busted(p)
                && ((p.side == 1 && price < i.limit_price)
                    || (p.side == -1 && price > i.limit_price))
        };
        if i.limit_price != 0 && limit_failed {
            restore_intent(s, &i)?;
            s.intents.remove(0);
            s.execute_ticket = add(s.execute_ticket, 1)?;
            record(
                s,
                Outcome {
                    ticket: i.ticket,
                    code: 5,
                    position_id: i.position_id,
                    price,
                    pnl: 0,
                    minted_credit: 0,
                },
            );
            continue;
        }
        s.intents.remove(0);
        let outcome = if i.kind == 0 {
            {
                if i.claim_id != 0 {
                    let c = s
                        .claims
                        .iter_mut()
                        .find(|c| c.id == i.claim_id)
                        .ok_or(ProgramError::NotEnoughAccountKeys)?;
                    c.reserved = sub(c.reserved, i.margin)?;
                }
                if i.claim_id == 0 {
                    s.users[u].pending_cash = sub(s.users[u].pending_cash, i.margin)?;
                }
                raise_lp_highwater(s)?;
                let id = s.next_position;
                s.next_position = add(id, 1)?;
                let m = &s.markets[i.market as usize];
                s.positions.push(Position {
                    id,
                    owner: i.owner,
                    market: i.market,
                    side: i.side,
                    margin: i.margin,
                    leverage: i.leverage,
                    entry: price,
                    debt_funded: i.claim_id != 0,
                    opened_at: now,
                    config_version: s.config_version,
                    position_multiplier_milli: m.position_multiplier_milli,
                    close_pending: false,
                    exit_ready_at: 0,
                    exit_limit_price: 0,
                });
                Outcome {
                    ticket: i.ticket,
                    code: 0,
                    position_id: id,
                    price,
                    pnl: 0,
                    minted_credit: 0,
                }
            }
        } else {
            let p = s
                .positions
                .iter()
                .position(|p| p.id == i.position_id)
                .ok_or_else(invalid)?;
            let bust = quote.busted(&s.positions[p]);
            settle_position(s, p, price, i.ticket, bust)?
        };
        s.execute_ticket = add(s.execute_ticket, 1)?;
        record(s, outcome);
    }
    Ok(())
}
/// Local fixtures may use seconds; any non-local deployment gets at least a day
/// for users to exit before a configuration change or upgrade takes effect.
pub fn min_governance_delay(local_mode: bool, devnet: bool) -> i64 {
    if local_mode {
        1
    } else if devnet {
        DEVNET_MIN_GOVERNANCE_DELAY
    } else {
        MIN_GOVERNANCE_DELAY
    }
}
pub fn process_instruction(id: &Pubkey, a: &[AccountInfo], data: &[u8]) -> ProgramResult {
    let ix = Instruction::try_from_slice(data).map_err(|_| ProgramError::InvalidInstructionData)?;
    if disabled_governance(&ix) {
        return Err(ProgramError::Custom(IMMUTABLE));
    }
    if let Instruction::Initialize {
        local_mode,
        devnet,
        sequencer,
        oracle_program,
        guardian,
        team,
        admins,
        governance_delay,
        markets,
    } = ix
    {
        return initialize(
            id,
            a,
            local_mode,
            devnet,
            sequencer,
            oracle_program,
            guardian,
            team,
            admins,
            governance_delay,
            markets,
        );
    }
    let mut s = read_state(account(a, 0)?)?;
    validate_common(id, a, &s)?;
    // Before sealing the program runs in full, but nothing permanent can happen:
    // the recovery paths that halt a market or open admission for good stay shut
    // while the upgrade authority can still repair whatever they would respond to.
    if !s.immutable
        && matches!(
            ix,
            Instruction::RemoveRelayer
                | Instruction::WindDownMarket { .. }
                | Instruction::SettleWindDown { .. }
                | Instruction::CancelWindDownIntent { .. }
        )
    {
        return Err(ProgramError::Custom(NOT_SEALED));
    }
    // Sealing revokes the upgrade authority, so these have no signer afterwards;
    // the flag is checked first so the refusal names the reason.
    if matches!(
        ix,
        Instruction::AddMarket { .. } | Instruction::ConfigureMarket { .. }
    ) {
        if s.immutable {
            return Err(ProgramError::Custom(IMMUTABLE));
        }
        if loader_authority(id, account(a, 12)?, account(a, 13)?)? != Some(*account(a, 1)?.key) {
            return Err(ProgramError::MissingRequiredSignature);
        }
    }
    let snapshot = storage::load(id, a, &mut s)?;
    let actor = *account(a, 1)?.key;
    let now = Clock::get()?.unix_timestamp;
    let mut rent_withdrawal = None;
    // Ticket an Enqueue was given, returned to the submitter once it lands.
    let mut admitted_ticket = None;
    match ix {
        Instruction::Deposit { amount } => {
            if s.paused || amount == 0 {
                return Err(invalid());
            }
            check_user_token(account(a, 6)?, &actor, &s.usdc_mint)?;
            let u = ensure_user(&mut s, actor)?;
            token_transfer(a, 6, 2, 1, amount, false, s.bump)?;
            s.users[u].available = add(s.users[u].available, amount)?;
        }
        Instruction::Withdraw { amount } => {
            check_user_token(account(a, 6)?, &actor, &s.usdc_mint)?;
            let u = user_index(&s, &actor)?;
            s.users[u].available = sub(s.users[u].available, amount)?;
            token_transfer(a, 2, 6, 0, amount, true, s.bump)?;
        }
        Instruction::SetDelegate {
            delegate,
            expiry,
            budget,
            max_leverage,
            market_mask,
        } => {
            if expiry <= now
                || expiry > now + 30 * 86400
                || max_leverage == 0
                || max_leverage > s.max_leverage
                || delegate == actor
            {
                return Err(invalid());
            }
            let u = user_index(&s, &actor)?;
            let x = &mut s.users[u];
            x.delegate = delegate;
            x.delegate_expiry = expiry;
            x.delegate_budget = budget;
            x.delegate_max_leverage = max_leverage;
            x.delegate_market_mask = market_mask;
            x.delegate_epoch = add(x.delegate_epoch, 1)?;
        }
        Instruction::RevokeDelegate => {
            let u = user_index(&s, &actor)?;
            let x = &mut s.users[u];
            x.delegate = Pubkey::default();
            x.delegate_expiry = 0;
            x.delegate_epoch = add(x.delegate_epoch, 1)?;
        }
        Instruction::Enqueue {
            // Advisory only: the ticket and market sequence are assigned when the
            // admission lands, so concurrent co-signed admissions never conflict.
            ticket: _,
            market_seq: _,
            owner,
            nonce,
            market,
            kind,
            side,
            margin,
            leverage,
            claim_id,
            position_id,
            expiry,
            limit_price,
            try_execute,
        } => {
            let seq = account(a, 8)?;
            if !s.permissionless {
                signer(seq)?;
                if seq.key != &s.sequencer {
                    return Err(ProgramError::MissingRequiredSignature);
                }
            }
            if market as usize >= s.markets.len() {
                return Err(invalid());
            }
            admissible(&s, market, now)?;
            if expiry <= now || expiry > now + MAX_INTENT_TTL || kind > 1 {
                return Err(invalid());
            }
            let u = user_index(&s, &owner)?;
            // The owner's next nonce only rises. An admission may land up to
            // NONCE_WINDOW below it, so an owner's parallel orders do not block
            // each other; the intent record keyed by (owner, nonce) cannot be
            // created while one with the same key is live or awaiting reclaim.
            let next = s.users[u].nonce;
            if nonce < next.saturating_sub(NONCE_WINDOW)
                || nonce > next.saturating_add(MAX_NONCE_GAP)
                || s.intents
                    .iter()
                    .any(|i| i.owner == owner && i.nonce == nonce)
            {
                return Err(ProgramError::Custom(BAD_SEQUENCE));
            }
            if actor != owner {
                let user = &s.users[u];
                if actor != user.delegate
                    || user.delegate_expiry <= now
                    || market >= 64
                    || user.delegate_market_mask & (1u64 << market) == 0
                    || leverage > user.delegate_max_leverage
                {
                    return Err(ProgramError::MissingRequiredSignature);
                }
            }
            let mut claim_priority = 0;
            if kind == 0 {
                if s.paused {
                    return Err(ProgramError::Custom(PAUSED));
                }
                if (side != 1 && side != -1)
                    || leverage == 0
                    || leverage > s.max_leverage
                    || margin < 1_000_000
                    || !notional_within_cap(margin, leverage)
                {
                    return Err(invalid());
                }
                if actor != owner {
                    // TODO(next program release): return this margin to the budget when the
                    // position it opened closes, so the budget caps a session's open margin
                    // rather than its lifetime total. Today only an unfilled open returns it
                    // (restore_intent), so one full-size trade ends a one-click session.
                    s.users[u].delegate_budget = sub(s.users[u].delegate_budget, margin)?;
                }
                if claim_id == 0 {
                    s.users[u].available = sub(s.users[u].available, margin)?;
                    s.users[u].pending_cash = add(s.users[u].pending_cash, margin)?;
                } else {
                    let c = s
                        .claims
                        .iter()
                        .position(|c| c.id == claim_id && c.owner == owner)
                        .ok_or_else(invalid)?;
                    claim_priority = s.claims[c].priority;
                    s.claims[c].amount = sub(s.claims[c].amount, margin)?;
                    s.claims[c].reserved = add(s.claims[c].reserved, margin)?;
                }
            } else {
                let p = s
                    .positions
                    .iter_mut()
                    .find(|p| p.id == position_id && p.owner == owner && p.market == market)
                    .ok_or_else(invalid)?;
                if p.close_pending || claim_id != 0 {
                    return Err(invalid());
                }
                p.close_pending = true;
            }
            let epoch = s.users[u].delegate_epoch;
            let ticket = s.next_ticket;
            let market_seq = s.markets[market as usize].next_seq;
            s.users[u].nonce = s.users[u].nonce.max(add(nonce, 1)?);
            s.next_ticket = add(s.next_ticket, 1)?;
            s.markets[market as usize].next_seq = add(s.markets[market as usize].next_seq, 1)?;
            admitted_ticket = Some(ticket);
            if !s.permissionless {
                s.last_sequenced_at = now;
            }
            s.intents.push(Intent {
                ticket,
                market_seq,
                owner,
                signer: actor,
                delegate_epoch: epoch,
                nonce,
                market,
                kind,
                side,
                margin,
                leverage,
                claim_id,
                position_id,
                expiry,
                limit_price,
                claim_priority,
                admitted_slot: Clock::get()?.slot,
            });
            emit_chain_event(&ChainEvent::Admission(s.intents.last().unwrap().clone()));
            if try_execute {
                drain(&mut s, a, 4)?;
            }
        }
        Instruction::Drain { max } => drain(&mut s, a, max)?,
        Instruction::Liquidate { position_id } => {
            let p = s
                .positions
                .iter()
                .position(|p| p.id == position_id)
                .ok_or_else(invalid)?;
            let pos = s.positions[p].clone();
            let quote = oracle(&mut s, a, pos.market)?;
            if !quote.busted(&pos) {
                return Err(invalid());
            }
            consume_mark(&mut s, pos.market, &quote)?;
            let o = settle_position(&mut s, p, quote.mark, 0, true)?;
            record(&mut s, o);
        }
        Instruction::Harvest { max } => {
            harvest(&mut s, max.min(MAX_BATCH as u16))?;
            sweep_terminal_residual(&mut s)?;
            if s.total_stake > 0 && s.reward_remainder > 0 {
                distribute(&mut s, 0)?;
            }
        }
        Instruction::MintRewards => {
            check_user_token(account(a, 7)?, &actor, &s.pencil_mint)?;
            let u = user_index(&s, &actor)?;
            let amount = s.users[u].token_credit;
            s.users[u].token_credit = 0;
            let tokenix = spl_token::instruction::mint_to(
                &spl_token::id(),
                account(a, 3)?.key,
                account(a, 7)?.key,
                account(a, 0)?.key,
                &[],
                amount,
            )?;
            invoke_signed(
                &tokenix,
                &[
                    account(a, 3)?.clone(),
                    account(a, 7)?.clone(),
                    account(a, 0)?.clone(),
                    account(a, 5)?.clone(),
                ],
                &[&[b"state", &[s.bump]]],
            )?;
        }
        Instruction::Stake { amount } => {
            if amount == 0 {
                return Err(invalid());
            }
            check_user_token(account(a, 7)?, &actor, &s.pencil_mint)?;
            let u = ensure_user(&mut s, actor)?;
            settle_rewards(&mut s, u)?;
            token_transfer(a, 7, 4, 1, amount, false, s.bump)?;
            s.users[u].pending_stake = add(s.users[u].pending_stake, amount)?;
            s.users[u].stake_ready_at = now
                .checked_add(s.stake_warmup)
                .ok_or(ProgramError::ArithmeticOverflow)?;
            s.total_pending_stake = add(s.total_pending_stake, amount)?;
        }
        Instruction::Unstake { amount } => {
            check_user_token(account(a, 7)?, &actor, &s.pencil_mint)?;
            let u = user_index(&s, &actor)?;
            settle_rewards(&mut s, u)?;
            let pending = amount.min(s.users[u].pending_stake);
            s.users[u].pending_stake -= pending;
            s.total_pending_stake -= pending;
            if s.users[u].pending_stake == 0 {
                s.users[u].stake_ready_at = 0;
            }
            s.users[u].stake = sub(s.users[u].stake, amount - pending)?;
            s.total_stake = sub(s.total_stake, amount - pending)?;
            token_transfer(a, 4, 7, 0, amount, true, s.bump)?;
        }
        Instruction::ClaimStaker => {
            let u = user_index(&s, &actor)?;
            settle_rewards(&mut s, u)?;
            let amount = s.users[u].reward_credit;
            s.users[u].reward_credit = 0;
            s.fee_staker = sub(s.fee_staker, amount)?;
            s.users[u].available = add(s.users[u].available, amount)?;
        }
        Instruction::ClaimTeam { amount } => {
            if actor != s.team {
                return Err(ProgramError::MissingRequiredSignature);
            }
            let u = ensure_user(&mut s, actor)?;
            s.fee_team = sub(s.fee_team, amount)?;
            s.users[u].available = add(s.users[u].available, amount)?;
        }
        Instruction::Pause => {
            // Local fixtures retain a stop to exercise frozen settlement without
            // a week-long runtime wait. Real deployments retire this authority.
            if s.immutable && s.permissionless && !s.local_mode {
                return Err(ProgramError::Custom(IMMUTABLE));
            }
            if actor != s.guardian {
                return Err(ProgramError::MissingRequiredSignature);
            }
            // During the trial the upgrade authority can lift an open pause.
            s.paused = true;
            if s.immutable {
                for m in &mut s.markets {
                    m.halted = true;
                }
            }
        }
        Instruction::Seal => seal(id, a, &mut s, now)?,
        Instruction::RemoveRelayer => {
            remove_relayer(&mut s, now)?
        }
        Instruction::SequencerHeartbeat => {
            if actor != s.sequencer {
                return Err(ProgramError::MissingRequiredSignature);
            }
            if !s.permissionless {
                s.last_sequenced_at = now;
            }
        }
        Instruction::CheckpointOracle { market } => {
            let q = oracle(&mut s, a, market)?;
            consume_mark(&mut s, market, &q)?;
        }
        Instruction::WindDownMarket { market } => {
            wind_down_market(&mut s, market, now, Clock::get()?.slot)?
        }
        Instruction::SettleWindDown { position_id } => {
            let o = settle_wind_down(&mut s, position_id)?;
            record(&mut s, o);
        }
        Instruction::CancelWindDownIntent { ticket } => cancel_wind_down_intent(&mut s, ticket)?,
        Instruction::ActivateStake => {
            let u = user_index(&s, &actor)?;
            if s.users[u].pending_stake == 0 || now < s.users[u].stake_ready_at {
                return Err(invalid());
            }
            settle_rewards(&mut s, u)?;
            let amount = s.users[u].pending_stake;
            s.users[u].pending_stake = 0;
            s.users[u].stake_ready_at = 0;
            s.users[u].stake = add(s.users[u].stake, amount)?;
            s.total_stake = add(s.total_stake, amount)?;
            s.total_pending_stake = sub(s.total_pending_stake, amount)?;
        }
        Instruction::RequestExit {
            position_id,
            limit_price,
        } => {
            let p = s
                .positions
                .iter_mut()
                .find(|p| p.id == position_id && p.owner == actor)
                .ok_or_else(invalid)?;
            p.exit_ready_at = now
                .checked_add(EXIT_DELAY)
                .ok_or(ProgramError::ArithmeticOverflow)?;
            p.exit_limit_price = limit_price;
        }
        Instruction::EnqueueExit { position_id } => {
            // Owner's prior onchain request authorizes these immutable terms.
            // Any keeper can publish it after the delay, even if the sequencer is live but censoring.
            let p = s
                .positions
                .iter_mut()
                .find(|p| p.id == position_id)
                .ok_or_else(invalid)?;
            if p.exit_ready_at == 0 || now < p.exit_ready_at || p.close_pending {
                return Err(invalid());
            }
            let exit_market = p.market;
            admissible(&s, exit_market, now)?;
            let p = s
                .positions
                .iter_mut()
                .find(|p| p.id == position_id)
                .ok_or_else(invalid)?;
            p.close_pending = true;
            p.exit_ready_at = 0;
            let (owner, market, side, limit) = (p.owner, p.market, p.side, p.exit_limit_price);
            let u = user_index(&s, &owner)?;
            let i = Intent {
                ticket: s.next_ticket,
                market_seq: s.markets[market as usize].next_seq,
                owner,
                signer: owner,
                delegate_epoch: 0,
                nonce: s.users[u].nonce,
                market,
                kind: 1,
                side,
                margin: 0,
                leverage: 0,
                claim_id: 0,
                position_id,
                expiry: now + MAX_INTENT_TTL,
                limit_price: limit,
                claim_priority: 0,
                admitted_slot: Clock::get()?.slot,
            };
            s.users[u].nonce = add(s.users[u].nonce, 1)?;
            s.next_ticket = add(s.next_ticket, 1)?;
            s.markets[market as usize].next_seq = add(s.markets[market as usize].next_seq, 1)?;
            emit_chain_event(&ChainEvent::Admission(i.clone()));
            s.intents.push(i);
            s.intents.sort_by_key(|i| i.ticket);
            drain(&mut s, a, 4)?;
        }
        Instruction::LiquidateBatch {
            market,
            position_ids,
        } => {
            if position_ids.is_empty() || position_ids.len() > MAX_BATCH {
                return Err(invalid());
            }
            let quote = oracle(&mut s, a, market)?;
            consume_mark(&mut s, market, &quote)?;
            for position_id in position_ids {
                if let Some(p) = s.positions.iter().position(|p| p.id == position_id) {
                    if s.positions[p].market != market {
                        return Err(invalid());
                    }
                    if quote.busted(&s.positions[p]) {
                        let o = settle_position(&mut s, p, quote.mark, 0, true)?;
                        record(&mut s, o);
                    }
                }
            }
        }
        Instruction::ReclaimRecord { record } => storage::reclaim(id, a, &record, s.claim_head)?,
        Instruction::Propose { .. }
        | Instruction::Approve { .. }
        | Instruction::Apply { .. }
        | Instruction::ProposeUpgrade { .. }
        | Instruction::ApplyUpgrade { .. }
        | Instruction::ProposeRotation { .. }
        | Instruction::ApplyRotation { .. }
        | Instruction::SetGovernanceDelay { .. } => {
            return Err(ProgramError::Custom(IMMUTABLE));
        }
        // Both are authorised above: unsealed, and signed by the upgrade authority.
        Instruction::AddMarket { mut market } => {
            if market.id as usize != s.markets.len() || s.markets.len() >= 8 {
                return Err(invalid());
            }
            if s.markets.first().map(|m| m.fill_delay_slots) != Some(market.fill_delay_slots) {
                return Err(invalid());
            }
            // The same checks a market passes at initialization, applied to this one alone.
            let mut fresh = market.clone();
            fresh.id = 0;
            validate_config(
                s.local_mode,
                s.devnet,
                &s.oracle_program,
                s.governance_delay,
                &[fresh],
            )?;
            market.version = s.config_version;
            s.markets.push(market);
        }
        Instruction::ConfigureMarket {
            market,
            clear_pause,
        } => {
            let current = s.markets.get(market.id as usize).ok_or_else(invalid)?;
            // Prices already stored are in this market's ticks, so the tick stays.
            if current.halted
                || market.tick_price_micro != current.tick_price_micro
                || s.markets.first().map(|m| m.fill_delay_slots) != Some(market.fill_delay_slots)
            {
                return Err(invalid());
            }
            let mut fresh = market.clone();
            fresh.id = 0;
            validate_config(
                s.local_mode,
                s.devnet,
                &s.oracle_program,
                s.governance_delay,
                &[fresh],
            )?;
            s.config_version = add(s.config_version, 1)?;
            let version = s.config_version;
            let m = &mut s.markets[market.id as usize];
            m.oracle_accounts = market.oracle_accounts;
            m.position_multiplier_milli = market.position_multiplier_milli;
            m.max_spread_bps = market.max_spread_bps;
            m.max_deviation_bps = market.max_deviation_bps;
            m.max_age_slots = market.max_age_slots;
            m.fill_delay_slots = market.fill_delay_slots;
            m.fill_window_slots = market.fill_window_slots;
            m.version = version;
            if clear_pause {
                s.paused = false;
            }
        }
        Instruction::WithdrawRent { lamports } => {
            user_index(&s, &actor)?;
            rent_withdrawal = Some(lamports);
        }
        Instruction::Initialize { .. } => unreachable!(),
    }
    storage::save(id, a, &mut s, snapshot)?;
    if let Some(lamports) = rent_withdrawal {
        storage::withdraw_float(id, a, lamports)?;
    }
    let v = unpack_token(account(a, 2)?)?;
    let st = unpack_token(account(a, 4)?)?;
    if v.amount < expected_vault(&s)? || st.amount < add(s.total_stake, s.total_pending_stake)? {
        return Err(ProgramError::InvalidAccountData);
    }
    save_state(account(a, 0)?, &s)?;
    if let Some(ticket) = admitted_ticket {
        // Last, after every CPI: the submitter reads the landed ticket here.
        set_return_data(&ticket.to_le_bytes());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn governance_delay_floor_applies_outside_local_mode() {
        assert_eq!(min_governance_delay(true, false), 1);
        assert_eq!(min_governance_delay(false, true), 3);
        assert_eq!(min_governance_delay(false, false), 86_400);
    }
    fn market(oracle_accounts: Vec<Pubkey>, delay: u16, window: u16) -> Market {
        Market {
            id: 0,
            symbol: *b"BTC\0\0\0\0\0",
            oracle_accounts,
            tick_price_micro: 1_000_000,
            position_multiplier_milli: 814_598,
            max_spread_bps: 0,
            max_deviation_bps: 50,
            max_age_slots: 10,
            next_seq: 1,
            version: 1,
            fill_delay_slots: delay,
            fill_window_slots: window,
            last_mark_slot: 0,
            last_mark: 0,
            last_mark_at: 0,
            halted: false,
            long_units: 0,
            short_notional: 0,
            quote_read_slot: 0,
            quote_version: 0,
            quote_mark: 0,
            quote_mark_slot: 0,
            quote_index_slot: 0,
            quote_status: 0,
        }
    }
    #[test]
    fn deployment_modes_pin_oracle_delay_and_fill_timing() {
        let hawkeye: Pubkey = HAWKEYE_PROGRAM.parse().unwrap();
        let phoenix: Pubkey = PHOENIX_PROGRAM.parse().unwrap();
        let stub = Pubkey::new_unique();
        let live = |n: usize| {
            let mut v = vec![phoenix];
            v.extend((1..n).map(|_| Pubkey::new_unique()));
            v
        };
        // Mainnet: real Hawkeye, Phoenix accounts, a day of delay, delayed fills.
        assert!(validate_config(false, false, &hawkeye, 86_400, &[market(live(7), 3, 12)]).is_ok());
        assert!(
            validate_config(false, false, &hawkeye, 86_399, &[market(live(7), 3, 12)]).is_err()
        );
        assert!(validate_config(false, false, &stub, 86_400, &[market(live(7), 3, 12)]).is_err());
        assert!(
            validate_config(false, false, &hawkeye, 86_400, &[market(live(7), 0, 12)]).is_err()
        );
        assert!(
            validate_config(false, false, &hawkeye, 86_400, &[market(live(6), 3, 12)]).is_err()
        );
        // Devnet: never the real Hawkeye, 3 s floor, delayed fills required.
        assert!(validate_config(false, true, &stub, 60, &[market(vec![stub], 3, 12)]).is_ok());
        assert!(validate_config(false, true, &hawkeye, 60, &[market(vec![stub], 3, 12)]).is_err());
        assert!(validate_config(false, true, &stub, 2, &[market(vec![stub], 3, 12)]).is_err());
        assert!(validate_config(false, true, &stub, 60, &[market(vec![stub], 0, 12)]).is_err());
        // Local fixtures may fill immediately; local and devnet are exclusive.
        assert!(validate_config(true, false, &stub, 1, &[market(vec![stub], 0, 1)]).is_ok());
        assert!(validate_config(true, true, &stub, 60, &[market(vec![stub], 3, 12)]).is_err());
        assert!(validate_config(true, false, &hawkeye, 1, &[market(vec![stub], 0, 1)]).is_err());
    }
    #[test]
    fn fill_windows_are_bounded_and_nonempty() {
        assert!(validate_fill(false, 3, 12).is_ok());
        assert!(validate_fill(false, 3, 0).is_err());
        assert!(validate_fill(false, MAX_FILL_DELAY_SLOTS + 1, 12).is_err());
        assert!(validate_fill(false, 3, MAX_FILL_WINDOW_SLOTS + 1).is_err());
        assert!(validate_fill(true, 0, 1).is_ok());
    }
}
