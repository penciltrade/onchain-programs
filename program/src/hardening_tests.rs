//! Regression tests for the pre-seal hardening changes: queue-head handling,
//! oracle bounds, admission freshness, and the staker profit-share rules.
use crate::{
    ledger::{self, ensure_user, expected_vault, settle_position, settle_rewards},
    math::{self, USDC},
    recovery_tests::SYSCALLS,
    types::*,
    *,
};
use solana_program::{
    program_error::ProgramError,
    program_option::COption,
    program_stubs::{set_syscall_stubs, SyscallStubs},
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

const PRICE: u64 = 80_000_000_000;
/// Oracle calls made since the chain in use was set up.
static ORACLE_CALLS: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy)]
struct Feed {
    mark: u64,
    index: u64,
    mark_slot: u64,
    index_slot: u64,
}
fn feed(mark: u64, slot: u64) -> Feed {
    Feed {
        mark,
        index: mark,
        mark_slot: slot,
        index_slot: slot,
    }
}
/// A chain with a clock and an oracle program answering Hawkeye's view_bbo.
struct Chain {
    clock: Clock,
    oracle: Pubkey,
    feeds: Vec<(Pubkey, Feed)>,
    returned: Mutex<Option<Vec<u8>>>,
}
impl SyscallStubs for Chain {
    fn sol_get_clock_sysvar(&self, address: *mut u8) -> u64 {
        unsafe {
            address.cast::<Clock>().write(self.clock.clone());
        }
        0
    }
    fn sol_get_rent_sysvar(&self, address: *mut u8) -> u64 {
        unsafe {
            address.cast::<Rent>().write(Rent::default());
        }
        0
    }
    fn sol_invoke_signed(
        &self,
        ix: &SolIx,
        _infos: &[AccountInfo],
        _seeds: &[&[&[u8]]],
    ) -> ProgramResult {
        if ix.program_id == self.oracle {
            ORACLE_CALLS.fetch_add(1, Ordering::SeqCst);
            let first = ix.accounts[0].pubkey;
            let f = self
                .feeds
                .iter()
                .find(|(k, _)| *k == first)
                .map(|(_, f)| *f);
            *self.returned.lock().unwrap() = f.map(|f| {
                let mut d = vec![0u8; 64];
                d[..8].copy_from_slice(&[0x71, 0x41, 0xa7, 0x1f, 0xa3, 0x1f, 0xca, 0xef]);
                d[8..10].copy_from_slice(&1u16.to_le_bytes());
                d[32..40].copy_from_slice(&f.mark.to_le_bytes());
                d[40..48].copy_from_slice(&f.index.to_le_bytes());
                d[48..56].copy_from_slice(&f.mark_slot.to_le_bytes());
                d[56..64].copy_from_slice(&f.index_slot.to_le_bytes());
                d
            });
        }
        Ok(())
    }
    fn sol_get_return_data(&self) -> Option<(Pubkey, Vec<u8>)> {
        self.returned
            .lock()
            .unwrap()
            .clone()
            .map(|d| (self.oracle, d))
    }
}
struct Restore(Option<Box<dyn SyscallStubs>>);
impl Drop for Restore {
    fn drop(&mut self) {
        if let Some(prior) = self.0.take() {
            set_syscall_stubs(prior);
        }
    }
}
fn chain(s: &State, slot: u64, now: i64, feeds: &[(u16, Feed)]) -> Box<Chain> {
    ORACLE_CALLS.store(0, Ordering::SeqCst);
    Box::new(Chain {
        clock: Clock {
            slot,
            unix_timestamp: now,
            ..Clock::default()
        },
        oracle: s.oracle_program,
        feeds: feeds
            .iter()
            .map(|(m, f)| (s.markets[*m as usize].oracle_accounts[0], *f))
            .collect(),
        returned: Mutex::new(None),
    })
}
/// What a caller of Drain or CheckpointOracle chose to supply.
#[derive(Default)]
struct Supplied {
    /// Markets whose oracle accounts are in the transaction.
    markets: Vec<u16>,
    /// Position records in the transaction.
    positions: Vec<u64>,
    /// Pinned oracle accounts that no longer belong to the oracle program.
    foreign: Vec<Pubkey>,
}
/// Run `f` against the real program code with a chain at `slot` whose oracle
/// answers `feeds` (one per market that has a readable quote).
fn on_chain<T>(
    s: &State,
    slot: u64,
    now: i64,
    feeds: &[(u16, Feed)],
    supplied: &Supplied,
    f: impl FnOnce(&[AccountInfo]) -> T,
) -> T {
    let _lock = SYSCALLS.lock().unwrap_or_else(|e| e.into_inner());
    let _restore = Restore(Some(set_syscall_stubs(chain(s, slot, now, feeds))));
    let program = Pubkey::new_unique();
    let system = solana_program::system_program::id();
    let mut keys = vec![Pubkey::new_unique(); 12];
    let mut owners = vec![system; 12];
    owners[0] = program;
    keys[9] = s.oracle_program;
    for m in &supplied.markets {
        for k in &s.markets[*m as usize].oracle_accounts {
            keys.push(*k);
            owners.push(if supplied.foreign.contains(k) {
                system
            } else {
                s.oracle_program
            });
        }
    }
    for id in &supplied.positions {
        keys.push(
            storage::address(&program, storage::POSITION, &id.to_le_bytes())
                .unwrap()
                .0,
        );
        owners.push(program);
    }
    let mut lamports = vec![1u64; keys.len()];
    let mut data: Vec<Vec<u8>> = keys.iter().map(|_| Vec::new()).collect();
    let accounts: Vec<AccountInfo> = keys
        .iter()
        .zip(owners.iter())
        .zip(lamports.iter_mut())
        .zip(data.iter_mut())
        .enumerate()
        .map(|(n, (((key, owner), balance), bytes))| {
            AccountInfo::new(key, false, false, balance, bytes, owner, n == 9, 0)
        })
        .collect();
    f(&accounts)
}
fn market(id: u16) -> Market {
    Market {
        id,
        symbol: *b"BTC\0\0\0\0\0",
        oracle_accounts: vec![Pubkey::new_unique(), Pubkey::new_unique()],
        tick_price_micro: 1,
        position_multiplier_milli: 814_598,
        max_spread_bps: 0,
        max_deviation_bps: 300,
        max_age_slots: 30,
        next_seq: 1,
        version: 1,
        fill_delay_slots: 10,
        fill_window_slots: 50,
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
/// A sealed, non-local deployment with two markets and delayed fills.
fn live() -> State {
    let mut s = ledger::tests::state();
    s.local_mode = false;
    s.devnet = true;
    s.markets = vec![market(0), market(1)];
    s
}
const ADMITTED: u64 = 100;
const OPENS: u64 = ADMITTED + 10;
/// Queue an open of $100 at 10x, already reserved from the owner's cash.
fn queue_open(s: &mut State, owner: Pubkey, market: u16) -> u64 {
    let u = ensure_user(s, owner).unwrap();
    s.users[u].pending_cash += 100 * USDC;
    let ticket = s.next_ticket;
    s.next_ticket += 1;
    s.intents.push(Intent {
        ticket,
        market_seq: 1,
        owner,
        signer: owner,
        delegate_epoch: 0,
        nonce: ticket,
        market,
        kind: 0,
        side: 1,
        margin: 100 * USDC,
        leverage: 10,
        claim_id: 0,
        position_id: 0,
        expiry: 1_030,
        limit_price: 0,
        claim_priority: 0,
        admitted_slot: ADMITTED,
    });
    ticket
}
// Capture the process-wide counter while on_chain still holds the syscall lock.
fn drain_at(
    s: &mut State,
    slot: u64,
    feeds: &[(u16, Feed)],
    markets: &[u16],
) -> Result<usize, ProgramError> {
    let supplied = Supplied {
        markets: markets.to_vec(),
        ..Supplied::default()
    };
    let snapshot = s.clone();
    on_chain(&snapshot, slot, 1_000, feeds, &supplied, |a| {
        drain(s, a, 16)?;
        Ok(ORACLE_CALLS.load(Ordering::SeqCst))
    })
}
fn outcome(s: &State, ticket: u64) -> Option<u8> {
    s.outcomes
        .iter()
        .find(|o| o.ticket == ticket)
        .map(|o| o.code)
}

#[test]
fn revoking_a_delegate_cannot_withdraw_an_order_already_queued() {
    let mut s = live();
    let owner = Pubkey::new_unique();
    let delegate = Pubkey::new_unique();
    let ticket = queue_open(&mut s, owner, 0);
    s.intents[0].signer = delegate;
    s.intents[0].delegate_epoch = 3;
    // RevokeDelegate, landed after admission and before the fill window.
    s.users[0].delegate = Pubkey::default();
    s.users[0].delegate_expiry = 0;
    s.users[0].delegate_epoch = 4;
    drain_at(&mut s, OPENS + 1, &[(0, feed(PRICE, OPENS))], &[0]).unwrap();
    assert_eq!(outcome(&s, ticket), Some(0), "the order fills at the mark");
    assert_eq!(s.positions.len(), 1);
    assert_eq!(s.positions[0].entry, PRICE);
    assert_eq!(s.users[0].pending_cash, 0);
    assert_eq!(s.users[0].available, 0);
    // A new delegate session does not rescue it either.
    let mut s = live();
    let ticket = queue_open(&mut s, owner, 0);
    s.intents[0].signer = delegate;
    s.intents[0].delegate_epoch = 3;
    s.users[0].delegate = Pubkey::new_unique();
    s.users[0].delegate_expiry = 5_000;
    s.users[0].delegate_epoch = 4;
    drain_at(&mut s, OPENS + 1, &[(0, feed(PRICE, OPENS))], &[0]).unwrap();
    assert_eq!(outcome(&s, ticket), Some(0));
}

#[test]
fn rejected_quote_ends_the_head_once_its_window_is_open_and_frees_other_markets() {
    let mut s = live();
    let a = Pubkey::new_unique();
    let b = Pubkey::new_unique();
    let stuck = queue_open(&mut s, a, 0);
    let behind = queue_open(&mut s, b, 1);
    // Market 0's mark is 5% from its index; market 1 is healthy.
    let bad = Feed {
        index: PRICE * 100 / 105,
        ..feed(PRICE, OPENS)
    };
    let feeds = [(0, bad), (1, feed(PRICE, OPENS))];
    // Before the window opens the head could not have filled anyway: wait.
    drain_at(&mut s, OPENS - 1, &feeds, &[0, 1]).unwrap();
    assert_eq!(s.intents.len(), 2);
    assert_eq!(s.execute_ticket, stuck);
    assert!(s.outcomes.is_empty());
    // Once it is open the unpriceable head ends, and the next market fills in
    // the same call instead of waiting out the head's expiry.
    drain_at(&mut s, OPENS, &feeds, &[0, 1]).unwrap();
    assert_eq!(outcome(&s, stuck), Some(6));
    assert_eq!(outcome(&s, behind), Some(0));
    assert!(s.intents.is_empty());
    assert_eq!(s.execute_ticket, behind + 1);
    assert_eq!(s.users[0].pending_cash, 0);
    assert_eq!(s.users[0].available, 100 * USDC, "reservation returned");
    assert_eq!(s.positions.len(), 1);
    assert_eq!(s.positions[0].owner, b);
    assert_eq!(
        s.markets[0].last_mark, 0,
        "a rejected quote is never consumed"
    );
}

#[test]
fn stale_or_malformed_quotes_end_the_head_the_same_way() {
    // A mark older than the market's maximum age.
    let mut s = live();
    let ticket = queue_open(&mut s, Pubkey::new_unique(), 0);
    drain_at(&mut s, OPENS + 40, &[(0, feed(PRICE, OPENS + 9))], &[0]).unwrap();
    assert_eq!(outcome(&s, ticket), Some(6));
    // An oracle that answers with no usable data.
    let mut s = live();
    let ticket = queue_open(&mut s, Pubkey::new_unique(), 0);
    drain_at(&mut s, OPENS, &[], &[0]).unwrap();
    assert_eq!(outcome(&s, ticket), Some(6));
    // A pinned oracle account that changed owner.
    let mut s = live();
    let ticket = queue_open(&mut s, Pubkey::new_unique(), 0);
    let supplied = Supplied {
        markets: vec![0],
        foreign: vec![s.markets[0].oracle_accounts[1]],
        ..Supplied::default()
    };
    let snapshot = s.clone();
    on_chain(
        &snapshot,
        OPENS,
        1_000,
        &[(0, feed(PRICE, OPENS))],
        &supplied,
        |a| drain(&mut s, a, 16),
    )
    .unwrap();
    assert_eq!(outcome(&s, ticket), Some(6));
    assert_eq!(s.users[0].available, 100 * USDC);
}

#[test]
fn head_waits_when_the_drainer_or_a_same_slot_mark_is_the_only_obstacle() {
    // Oracle accounts left out of the transaction: the drainer's choice, so it
    // must never end somebody's order.
    let mut s = live();
    let ticket = queue_open(&mut s, Pubkey::new_unique(), 0);
    drain_at(&mut s, OPENS + 5, &[(0, feed(PRICE, OPENS + 5))], &[]).unwrap();
    assert_eq!(s.intents.len(), 1);
    assert_eq!(s.execute_ticket, ticket);
    assert!(s.outcomes.is_empty());
    // A second, different mark for a slot already consumed. Anyone can cause
    // this by moving the book within a slot, so it waits for the next slot.
    s.markets[0].last_mark = PRICE;
    s.markets[0].last_mark_slot = OPENS + 5;
    s.markets[0].last_mark_at = 1_000;
    drain_at(
        &mut s,
        OPENS + 5,
        &[(0, feed(PRICE + 1_000_000, OPENS + 5))],
        &[0],
    )
    .unwrap();
    assert_eq!(s.intents.len(), 1);
    assert!(s.outcomes.is_empty());
    assert_eq!(s.users[0].pending_cash, 100 * USDC);
    // The next slot's mark fills it.
    drain_at(
        &mut s,
        OPENS + 6,
        &[(0, feed(PRICE + 1_000_000, OPENS + 6))],
        &[0],
    )
    .unwrap();
    assert_eq!(outcome(&s, ticket), Some(0));
    assert_eq!(s.positions[0].entry, PRICE + 1_000_000);
}

#[test]
fn immediate_fill_fixtures_keep_waiting_on_a_rejected_quote() {
    // Local fixtures fill with no delay, so there is no window to anchor on.
    let mut s = live();
    s.local_mode = true;
    s.devnet = false;
    s.markets[0].fill_delay_slots = 0;
    let ticket = queue_open(&mut s, Pubkey::new_unique(), 0);
    let bad = Feed {
        index: PRICE / 2,
        ..feed(PRICE, OPENS + 5)
    };
    drain_at(&mut s, OPENS + 5, &[(0, bad)], &[0]).unwrap();
    assert_eq!(s.execute_ticket, ticket);
    assert!(s.outcomes.is_empty());
}

fn read(s: &State, slot: u64, f: Feed) -> Result<u64, ProgramError> {
    let supplied = Supplied {
        markets: vec![0],
        ..Supplied::default()
    };
    on_chain(s, slot, 1_000, &[(0, f)], &supplied, |a| {
        oracle(&mut s.clone(), a, 0).map(|q| q.mark)
    })
}
#[test]
fn mark_must_stay_within_thirty_percent_of_the_last_accepted_mark() {
    let bad = Err(ProgramError::Custom(BAD_ORACLE));
    let mut s = live();
    // The first mark of a market has nothing to be compared with.
    assert_eq!(read(&s, 200, feed(PRICE, 200)), Ok(PRICE));
    checkpoint(&mut s, 0, PRICE, 200, 1_000).unwrap();
    assert_eq!(
        read(&s, 201, feed(PRICE * 13 / 10, 201)),
        Ok(PRICE * 13 / 10)
    );
    assert_eq!(read(&s, 201, feed(PRICE * 7 / 10, 201)), Ok(PRICE * 7 / 10));
    assert_eq!(read(&s, 201, feed(PRICE * 13 / 10 + 1, 201)), bad);
    assert_eq!(read(&s, 201, feed(PRICE * 7 / 10 - 1, 201)), bad);
    // A thousandfold change of price scale upstream moves mark and index
    // together, so only this band catches it.
    assert_eq!(read(&s, 201, feed(PRICE * 1_000, 201)), bad);
    assert_eq!(read(&s, 201, feed(PRICE / 1_000, 201)), bad);
    // A mark too large to price is a rejected quote, not a failed transaction,
    // even as a market's first mark, which the band does not cover.
    s.markets[0].tick_price_micro = 1_000_000;
    assert_eq!(read(&s, 201, feed(u64::MAX, 201)), bad);
    let mut unpriced = live();
    unpriced.markets[0].tick_price_micro = 2;
    assert_eq!(read(&unpriced, 201, feed(u64::MAX, 201)), bad);
    s.markets[0].tick_price_micro = 1;
    // Outside the queue, a same-slot conflict is reported as a bad quote.
    assert_eq!(read(&s, 200, feed(PRICE + 1, 200)), bad);
    // Local fixtures step prices freely.
    s.local_mode = true;
    s.devnet = false;
    assert_eq!(read(&s, 201, feed(PRICE * 2, 201)), Ok(PRICE * 2));
    assert!(mark_within_band(0, u64::MAX as u128));
    assert!(mark_within_band(u64::MAX / 2, u64::MAX as u128 / 2));
    assert!(!mark_within_band(1, u64::MAX as u128));
}

/// A record account handed to an instruction.
enum Rec {
    User(User),
    Position(Position),
}
/// What one instruction is run with through the real entrypoint.
struct Call<'a> {
    actor: Pubkey,
    slot: u64,
    now: i64,
    feeds: &'a [(u16, Feed)],
    /// Markets whose oracle accounts are in the transaction.
    markets: &'a [u16],
    records: &'a [Rec],
}
impl Default for Call<'_> {
    fn default() -> Self {
        Call {
            actor: Pubkey::new_unique(),
            slot: 300,
            now: 1_000,
            feeds: &[],
            markets: &[],
            records: &[],
        }
    }
}
/// Run `ix` against root state `s` (whose totals must cover `records`) and
/// return the root as the instruction left it.
fn enter(s: &State, ix: Instruction, call: Call) -> Result<State, ProgramError> {
    use spl_token::state::AccountState;
    let _lock = SYSCALLS.lock().unwrap_or_else(|e| e.into_inner());
    let _restore = Restore(Some(set_syscall_stubs(chain(
        s, call.slot, call.now, call.feeds,
    ))));
    let id = Pubkey::new_unique();
    let (root, bump) = Pubkey::find_program_address(&[b"state"], &id);
    let mut s = s.clone();
    s.bump = bump;
    // Existing fixtures supply a full position record without its aggregate.
    // Construct the valid root counters that an actual open would have saved.
    let mut supplied = s.clone();
    supplied.positions = call
        .records
        .iter()
        .filter_map(|r| match r {
            Rec::Position(p) => Some(p.clone()),
            _ => None,
        })
        .collect();
    for (m, e) in s
        .markets
        .iter_mut()
        .zip(ledger::loaded_exposures(&supplied).unwrap())
    {
        m.long_units = m.long_units.max(e.long_units);
        m.short_notional = m.short_notional.max(e.short_notional);
    }
    let system = solana_program::system_program::id();
    let token = spl_token::id();
    let mut keys = vec![
        root,
        call.actor,
        s.usdc_vault,
        s.pencil_mint,
        s.stake_vault,
        token,
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        s.sequencer,
        s.oracle_program,
        system,
        call.actor,
    ];
    let mut owners = vec![
        id, system, token, token, token, system, system, system, system, system, system, system,
    ];
    let mut data: Vec<Vec<u8>> = (0..12).map(|_| Vec::new()).collect();
    let bytes = borsh::to_vec(&s).unwrap();
    data[0] = vec![0; 8192];
    data[0][..4].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
    data[0][4..4 + bytes.len()].copy_from_slice(&bytes);
    let vault = |mint, amount| {
        let mut bytes = vec![0; TokenAccount::LEN];
        let account = TokenAccount {
            mint,
            owner: root,
            amount,
            delegate: COption::None,
            state: AccountState::Initialized,
            is_native: COption::None,
            delegated_amount: 0,
            close_authority: COption::None,
        };
        TokenAccount::pack(account, &mut bytes).unwrap();
        bytes
    };
    data[2] = vault(s.usdc_mint, expected_vault(&s).unwrap());
    data[4] = vault(s.pencil_mint, s.total_stake + s.total_pending_stake);
    data[3] = vec![0; Mint::LEN];
    Mint::pack(
        Mint {
            mint_authority: COption::Some(root),
            supply: 0,
            decimals: 6,
            is_initialized: true,
            freeze_authority: COption::None,
        },
        &mut data[3],
    )
    .unwrap();
    for m in call.markets {
        for k in &s.markets[*m as usize].oracle_accounts {
            keys.push(*k);
            owners.push(s.oracle_program);
            data.push(Vec::new());
        }
    }
    for r in call.records {
        let (kind, seed, body) = match r {
            Rec::User(u) => (
                storage::USER,
                u.owner.to_bytes().to_vec(),
                borsh::to_vec(u).unwrap(),
            ),
            Rec::Position(p) => (
                storage::POSITION,
                p.id.to_le_bytes().to_vec(),
                borsh::to_vec(p).unwrap(),
            ),
        };
        keys.push(storage::address(&id, kind, &seed).unwrap().0);
        owners.push(id);
        data.push([&[kind][..], &[0u8; 32][..], &body[..]].concat());
    }
    let mut lamports = vec![1_000_000_000u64; keys.len()];
    let accounts: Vec<AccountInfo> = keys
        .iter()
        .zip(owners.iter())
        .zip(lamports.iter_mut())
        .zip(data.iter_mut())
        .enumerate()
        .map(|(n, (((key, owner), balance), bytes))| {
            AccountInfo::new(
                key,
                n == 1 || n == 8 || n == 11,
                true,
                balance,
                bytes,
                owner,
                n == 5 || n == 9 || n == 10,
                0,
            )
        })
        .collect();
    process_instruction(&id, &accounts, &borsh::to_vec(&ix).unwrap())?;
    read_state(&accounts[0])
}
fn open_order(owner: Pubkey, margin: u64, leverage: u16) -> Instruction {
    Instruction::Enqueue {
        ticket: 0,
        market_seq: 0,
        owner,
        nonce: 0,
        market: 0,
        kind: 0,
        side: 1,
        margin,
        leverage,
        claim_id: 0,
        position_id: 0,
        expiry: 1_010,
        limit_price: 0,
        try_execute: false,
    }
}
const BAD: Result<State, ProgramError> = Err(ProgramError::Custom(BAD_ORACLE));

#[test]
fn checkpoint_and_liquidation_report_every_rejection_as_a_bad_quote() {
    let mut s = live();
    checkpoint(&mut s, 0, PRICE, 300, 1_000).unwrap();
    let run = |s: &State, ix: Instruction, f: Feed, records: &[Rec]| {
        let feeds = [(0, f)];
        enter(
            s,
            ix,
            Call {
                slot: f.mark_slot,
                feeds: &feeds,
                markets: &[0],
                records,
                ..Call::default()
            },
        )
    };
    let conflict = feed(PRICE + 5, 300);
    let checkpointed = |f: Feed| {
        run(&s, Instruction::CheckpointOracle { market: 0 }, f, &[]).map(|s| s.markets[0].last_mark)
    };
    let bad = Err(ProgramError::Custom(BAD_ORACLE));
    // An accepted mark is recorded. A second mark for the same slot and a mark
    // outside the band are both reported as error 100, never the internal 107.
    assert_eq!(checkpointed(conflict), bad);
    assert_eq!(checkpointed(feed(PRICE * 2, 301)), bad);
    assert_eq!(checkpointed(feed(PRICE + 5, 301)), Ok(PRICE + 5));
    let batch = Instruction::LiquidateBatch {
        market: 0,
        position_ids: vec![1],
    };
    assert_eq!(run(&s, batch, conflict, &[]).map(|_| ()), BAD.map(|_| ()));
    let mut p = pos(1, Pubkey::new_unique(), 100 * USDC, 1);
    p.entry = PRICE;
    let mut with_position = s.clone();
    with_position.totals.cash_margin = p.margin;
    let liquidate = Instruction::Liquidate { position_id: 1 };
    assert_eq!(
        run(&with_position, liquidate, conflict, &[Rec::Position(p)]).map(|_| ()),
        BAD.map(|_| ())
    );
}

#[test]
fn enqueue_and_enqueue_exit_refuse_a_market_without_a_recent_checkpoint() {
    let mut s = live();
    s.permissionless = true;
    checkpoint(&mut s, 0, PRICE, 300, 1_000).unwrap();
    let owner = Pubkey::new_unique();
    let order = |now| {
        enter(
            &s,
            open_order(owner, 10 * USDC, 2),
            Call {
                actor: owner,
                now,
                ..Call::default()
            },
        )
        .map(|_| ())
    };
    // Past the limit the market is refused before anything else is looked at.
    // Inside it the same call gets as far as the missing trading account.
    let late = 1_001 + ADMISSION_MARK_MAX_AGE;
    assert_eq!(order(late), BAD.map(|_| ()));
    assert_eq!(order(late - 1), Err(ProgramError::InvalidArgument));
    // The published exit of an existing position obeys the same rule.
    let mut p = pos(1, owner, 100 * USDC, 1);
    p.entry = PRICE;
    p.exit_ready_at = 900;
    s.totals.cash_margin = p.margin;
    let exit = |now| {
        enter(
            &s,
            Instruction::EnqueueExit { position_id: 1 },
            Call {
                now,
                records: &[Rec::Position(p.clone())],
                ..Call::default()
            },
        )
        .map(|_| ())
    };
    assert_eq!(exit(late), BAD.map(|_| ()));
    assert_eq!(exit(late - 1), Err(ProgramError::InvalidArgument));
}

#[test]
fn enqueue_refuses_an_open_above_the_notional_limit() {
    let mut s = ledger::tests::state();
    s.markets = vec![market(0)];
    let owner = Pubkey::new_unique();
    let mut funded = s.clone();
    let u = ensure_user(&mut funded, owner).unwrap();
    let user = funded.users[u].clone();
    let order = |margin, leverage| {
        enter(
            &s,
            open_order(owner, margin, leverage),
            Call {
                actor: owner,
                records: &[Rec::User(user.clone())],
                ..Call::default()
            },
        )
        .map(|_| ())
    };
    // Above the limit the terms are invalid. At the limit they are accepted
    // and the order gets as far as the owner's empty cash balance.
    assert_eq!(
        order(1_250 * USDC + 1, 200),
        Err(ProgramError::InvalidArgument)
    );
    assert_eq!(
        order(250_000 * USDC + 1, 1),
        Err(ProgramError::InvalidArgument)
    );
    assert_eq!(
        order(1_250 * USDC, 200),
        Err(ProgramError::InsufficientFunds)
    );
    assert_eq!(
        order(250_000 * USDC, 1),
        Err(ProgramError::InsufficientFunds)
    );
}

#[test]
fn a_run_of_intents_on_a_rejected_market_costs_one_oracle_call() {
    let mut s = live();
    let tickets: Vec<u64> = (0..6)
        .map(|_| queue_open(&mut s, Pubkey::new_unique(), 0))
        .collect();
    let bad = Feed {
        index: PRICE / 2,
        ..feed(PRICE, OPENS)
    };
    let snapshot = s.clone();
    let supplied = Supplied {
        markets: vec![0],
        ..Supplied::default()
    };
    let calls = on_chain(&snapshot, OPENS, 1_000, &[(0, bad)], &supplied, |a| {
        drain(&mut s, a, 16).unwrap();
        ORACLE_CALLS.load(Ordering::SeqCst)
    });
    assert_eq!(calls, 1);
    assert!(s.intents.is_empty());
    for (n, ticket) in tickets.iter().enumerate() {
        assert_eq!(outcome(&s, *ticket), Some(6));
        assert_eq!(s.users[n].available, 100 * USDC);
    }
}

#[test]
fn separate_drains_reuse_the_first_observation_in_a_slot() {
    let mut s = live();
    let first = Pubkey::new_unique();
    queue_open(&mut s, first, 0);
    drain_at(&mut s, OPENS, &[(0, feed(PRICE, OPENS))], &[0]).unwrap();
    assert_eq!(s.positions[0].entry, PRICE);
    assert_eq!(s.markets[0].quote_read_slot, OPENS);
    assert_eq!(s.markets[0].quote_mark_slot, OPENS);
    queue_open(&mut s, Pubkey::new_unique(), 0);
    let calls = drain_at(&mut s, OPENS, &[(0, feed(PRICE + 100, OPENS))], &[0]).unwrap();
    assert_eq!(calls, 0);
    assert_eq!(
        s.positions[1].entry, PRICE,
        "the first committed quote prices the whole slot"
    );
    queue_open(&mut s, Pubkey::new_unique(), 0);
    let calls = drain_at(
        &mut s,
        OPENS + 1,
        &[(0, feed(PRICE + 100, OPENS + 1))],
        &[0],
    )
    .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(s.positions[2].entry, PRICE + 100);
}

#[test]
fn a_rejected_committed_quote_is_also_reused_until_the_next_slot() {
    let mut s = live();
    queue_open(&mut s, Pubkey::new_unique(), 0);
    let bad = Feed {
        index: PRICE / 2,
        ..feed(PRICE, OPENS)
    };
    drain_at(&mut s, OPENS, &[(0, bad)], &[0]).unwrap();
    assert_eq!(s.markets[0].quote_status, 2);
    queue_open(&mut s, Pubkey::new_unique(), 0);
    let calls = drain_at(&mut s, OPENS, &[(0, feed(PRICE, OPENS))], &[0]).unwrap();
    assert_eq!(calls, 0);
    assert!(s.positions.is_empty());
    queue_open(&mut s, Pubkey::new_unique(), 0);
    let calls = drain_at(&mut s, OPENS + 1, &[(0, feed(PRICE, OPENS + 1))], &[0]).unwrap();
    assert_eq!(calls, 1);
    assert_eq!(s.positions.len(), 1);
}

#[test]
fn slot_cache_preserves_source_slots_and_survives_configuration_invalidation() {
    let mut s = live();
    let snapshot = s.clone();
    let supplied = Supplied {
        markets: vec![0, 1],
        ..Supplied::default()
    };
    on_chain(
        &snapshot,
        OPENS,
        1_000,
        &[(0, feed(PRICE, OPENS - 1)), (1, feed(PRICE, OPENS))],
        &supplied,
        |a| {
            let q = read_oracle(&mut s, a, 0).unwrap();
            assert_eq!(q.mark_slot, OPENS - 1);
            assert_eq!(q.index_slot, OPENS - 1);
            read_oracle(&mut s, a, 1).unwrap();
            assert_eq!(
                ORACLE_CALLS.load(Ordering::SeqCst),
                2,
                "each market gets one observation"
            );
            s.markets[0].version += 1;
            assert!(matches!(
                read_oracle(&mut s, a, 0),
                Err(ProgramError::Custom(BAD_ORACLE))
            ));
            assert_eq!(
                ORACLE_CALLS.load(Ordering::SeqCst),
                2,
                "reconfiguration cannot trigger a second read"
            );
        },
    );
    let snapshot = s.clone();
    on_chain(
        &snapshot,
        OPENS + 1,
        1_001,
        &[(0, feed(PRICE, OPENS + 1))],
        &supplied,
        |a| {
            read_oracle(&mut s, a, 0).unwrap();
            assert_eq!(ORACLE_CALLS.load(Ordering::SeqCst), 1);
            assert_eq!(s.markets[0].quote_version, s.markets[0].version);
        },
    );
}

#[test]
fn omitted_winning_positions_still_reserve_global_exposure() {
    let (mut s, trader) = house(1_000_000 * USDC);
    s.markets = vec![market(0)];
    s.markets[0].last_mark = 100_300_000;
    ensure_user(&mut s, trader).unwrap();
    s.positions.push(pos(1, trader, 2_500 * USDC, 1));
    s.positions.push(pos(2, trader, 2_500 * USDC, -1));
    let all = ledger::loaded_exposures(&s).unwrap();
    s.markets[0].long_units = all[0].long_units;
    s.markets[0].short_notional = all[0].short_notional;
    s.totals = ledger::loaded_totals(&s).unwrap();
    // Only the losing short record is supplied to this transaction.
    s.positions.remove(0);
    s.loaded_exposures = ledger::loaded_exposures(&s).unwrap();
    s.loaded_totals = ledger::loaded_totals(&s).unwrap();
    let before = s.house_cash;
    let shared = settle_shared(&mut s, 2, 100_300_000, false);
    assert_eq!(shared, 0);
    assert!(ledger::profit_reserve(&s).unwrap() > 250_000 * USDC);
    assert_eq!(s.house_cash, before + 735 * USDC);
    assert_eq!(
        ledger::current_exposures(&s).unwrap()[0].long_units,
        all[0].long_units
    );
    assert_eq!(ledger::current_exposures(&s).unwrap()[0].short_notional, 0);
}

#[test]
fn every_market_gets_the_same_fill_delay() {
    let stub = Pubkey::new_unique();
    let mut markets = vec![market(0), market(1)];
    assert!(validate_config(false, true, &stub, 60, &markets).is_ok());
    markets[1].fill_delay_slots = 11;
    assert!(validate_config(false, true, &stub, 60, &markets).is_err());
    markets[0].fill_delay_slots = 11;
    assert!(validate_config(false, true, &stub, 60, &markets).is_ok());
}

#[test]
fn configuration_bounds_position_multiplier_for_the_full_price_domain() {
    let stub = Pubkey::new_unique();
    let mut m = market(0);
    m.position_multiplier_milli = MAX_POSITION_MULTIPLIER_MILLI;
    assert!(validate_config(false, true, &stub, 60, &[m.clone()]).is_ok());
    m.position_multiplier_milli += 1;
    assert_eq!(
        validate_config(false, true, &stub, 60, &[m]),
        Err(ProgramError::InvalidArgument)
    );
}

#[test]
fn admission_requires_a_recent_checkpoint_outside_local_fixtures() {
    let mut s = live();
    let bad = Err(ProgramError::Custom(BAD_ORACLE));
    assert_eq!(admissible(&s, 0, 1_000), bad, "never priced");
    checkpoint(&mut s, 0, PRICE, 200, 1_000).unwrap();
    assert_eq!(admissible(&s, 0, 1_000), Ok(()));
    assert_eq!(admissible(&s, 0, 1_000 + ADMISSION_MARK_MAX_AGE), Ok(()));
    assert_eq!(admissible(&s, 0, 1_001 + ADMISSION_MARK_MAX_AGE), bad);
    assert_eq!(admissible(&s, 1, 1_000), bad, "each market on its own mark");
    s.markets[0].halted = true;
    assert_eq!(admissible(&s, 0, 1_000), Err(ProgramError::Custom(PAUSED)));
    assert!(admissible(&s, 2, 1_000).is_err());
    let mut local = live();
    local.local_mode = true;
    local.devnet = false;
    assert_eq!(admissible(&local, 0, 1_000_000), Ok(()));
}

/// Run Initialize through the real entrypoint with the given vault accounts.
/// Every check in `initialize` runs before its first CPI, which this chain
/// answers with a marker error: reaching it means the accounts were accepted.
const ACCEPTED: u32 = 4_242;
fn initialize_with(vault: impl Fn(&mut TokenAccount), stake: impl Fn(&mut TokenAccount)) -> u32 {
    initialize_with_terms(vault, stake, false, Pubkey::new_unique())
}
fn initialize_with_terms(
    vault: impl Fn(&mut TokenAccount),
    stake: impl Fn(&mut TokenAccount),
    mainnet: bool,
    usdc: Pubkey,
) -> u32 {
    use solana_program::bpf_loader_upgradeable;
    struct FirstCpi;
    impl SyscallStubs for FirstCpi {
        fn sol_get_rent_sysvar(&self, address: *mut u8) -> u64 {
            unsafe {
                address.cast::<Rent>().write(Rent::default());
            }
            0
        }
        fn sol_invoke_signed(
            &self,
            _ix: &SolIx,
            _infos: &[AccountInfo],
            _seeds: &[&[&[u8]]],
        ) -> ProgramResult {
            Err(ProgramError::Custom(ACCEPTED))
        }
    }
    let _lock = SYSCALLS.lock().unwrap_or_else(|e| e.into_inner());
    let _restore = Restore(Some(set_syscall_stubs(Box::new(FirstCpi))));
    let id = Pubkey::new_unique();
    let state = Pubkey::find_program_address(&[b"state"], &id).0;
    let programdata = Pubkey::find_program_address(&[id.as_ref()], &bpf_loader_upgradeable::id()).0;
    let payer = Pubkey::new_unique();
    let pencil = Pubkey::new_unique();
    let system = solana_program::system_program::id();
    let token = spl_token::id();
    let loader = bpf_loader_upgradeable::id();
    let keys = [
        payer,
        state,
        usdc,
        pencil,
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        token,
        system,
        id,
        programdata,
    ];
    let owners = [
        system, system, token, token, token, token, system, system, loader, loader,
    ];
    let mut data: Vec<Vec<u8>> = (0..10).map(|_| Vec::new()).collect();
    let mint = |authority| Mint {
        mint_authority: authority,
        supply: 0,
        decimals: 6,
        is_initialized: true,
        freeze_authority: COption::None,
    };
    data[2] = vec![0; Mint::LEN];
    Mint::pack(mint(COption::Some(payer)), &mut data[2]).unwrap();
    data[3] = vec![0; Mint::LEN];
    Mint::pack(mint(COption::Some(state)), &mut data[3]).unwrap();
    let account = |mint, change: &dyn Fn(&mut TokenAccount)| {
        let mut t = TokenAccount {
            mint,
            owner: state,
            amount: 0,
            delegate: COption::None,
            state: spl_token::state::AccountState::Initialized,
            is_native: COption::None,
            delegated_amount: 0,
            close_authority: COption::None,
        };
        change(&mut t);
        let mut bytes = vec![0; TokenAccount::LEN];
        TokenAccount::pack(t, &mut bytes).unwrap();
        bytes
    };
    data[4] = account(usdc, &vault);
    data[5] = account(pencil, &stake);
    data[8] = [2u32.to_le_bytes().as_slice(), programdata.as_ref()].concat();
    data[9] = [3u32.to_le_bytes().as_slice(), &[0; 8], &[1], payer.as_ref()].concat();
    let mut lamports = [1_000_000_000_000u64; 10];
    let accounts: Vec<AccountInfo> = keys
        .iter()
        .zip(owners.iter())
        .zip(lamports.iter_mut())
        .zip(data.iter_mut())
        .enumerate()
        .map(|(n, (((key, owner), balance), bytes))| {
            AccountInfo::new(key, n == 0, true, balance, bytes, owner, n == 8, 0)
        })
        .collect();
    let mut m = market(0);
    m.oracle_accounts = if mainnet {
        let mut keys = vec![PHOENIX_PROGRAM.parse().unwrap()];
        keys.extend((0..6).map(|_| Pubkey::new_unique()));
        keys
    } else {
        vec![Pubkey::new_unique()]
    };
    let ix = Instruction::Initialize {
        local_mode: false,
        devnet: !mainnet,
        sequencer: Pubkey::new_unique(),
        oracle_program: if mainnet {
            HAWKEYE_PROGRAM.parse().unwrap()
        } else {
            Pubkey::new_unique()
        },
        guardian: Pubkey::new_unique(),
        team: Pubkey::new_unique(),
        admins: [
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        ],
        governance_delay: if mainnet { MIN_GOVERNANCE_DELAY } else { 60 },
        markets: vec![m],
    };
    match process_instruction(&id, &accounts, &borsh::to_vec(&ix).unwrap()) {
        Err(ProgramError::Custom(code)) => code,
        Err(ProgramError::InvalidArgument) => 0,
        other => panic!("unexpected initialize result {other:?}"),
    }
}
#[test]
fn mainnet_initialization_pins_circle_usdc_before_any_cpi() {
    let keep = |_: &mut TokenAccount| {};
    assert_eq!(
        initialize_with_terms(keep, keep, true, MAINNET_USDC),
        ACCEPTED
    );
    assert_eq!(
        initialize_with_terms(keep, keep, true, Pubkey::new_unique()),
        0
    );
}
#[test]
fn initialize_refuses_a_vault_with_a_delegate_or_close_authority() {
    let other = COption::Some(Pubkey::new_unique());
    let keep = |_: &mut TokenAccount| {};
    assert_eq!(initialize_with(keep, keep), ACCEPTED);
    // Left over from before the account was handed to the state address: a
    // close authority could close the empty vault and stop every instruction.
    assert_eq!(initialize_with(|t| t.close_authority = other, keep), 0);
    assert_eq!(initialize_with(keep, |t| t.close_authority = other), 0);
    let approve = |t: &mut TokenAccount| {
        t.delegate = other;
        t.delegated_amount = u64::MAX;
    };
    assert_eq!(initialize_with(approve, keep), 0);
    assert_eq!(initialize_with(keep, approve), 0);
    let clean = TokenAccount {
        mint: Pubkey::new_unique(),
        owner: Pubkey::new_unique(),
        amount: 0,
        delegate: COption::None,
        state: spl_token::state::AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };
    assert!(vault_is_exclusive(&clean));
    assert!(!vault_is_exclusive(&TokenAccount {
        close_authority: other,
        ..clean
    }));
}

#[test]
fn position_notional_is_capped_at_a_quarter_million_dollars() {
    assert!(notional_within_cap(250_000 * USDC, 1));
    assert!(!notional_within_cap(250_000 * USDC + 1, 1));
    assert!(notional_within_cap(1_250 * USDC, 200));
    assert!(!notional_within_cap(1_250 * USDC + 1, 200));
    assert!(!notional_within_cap(u64::MAX, 200));
}

fn pos(id: u64, owner: Pubkey, margin: u64, side: i8) -> Position {
    Position {
        id,
        owner,
        market: 0,
        side,
        margin,
        leverage: 100,
        entry: 100_000_000,
        debt_funded: false,
        opened_at: 0,
        config_version: 1,
        position_multiplier_milli: 814_598,
        close_pending: false,
        exit_ready_at: 0,
        exit_limit_price: 0,
    }
}

#[test]
fn recovery_and_limit_constants_are_the_reviewed_values() {
    // These are fixed for the life of an immutable program; the SDK, keeper
    // and documents quote them. A change here must be a deliberate one.
    assert_eq!(SEQUENCER_TIMEOUT, 86_400);
    assert_eq!(ORACLE_TIMEOUT, 7 * 86_400);
    assert_eq!(ORACLE_MIN_SLOTS, 756_000);
    assert_eq!(MAX_MARK_MOVE_BPS, 3_000);
    assert_eq!(ADMISSION_MARK_MAX_AGE, 120);
    assert_eq!(MAX_NOTIONAL, 250_000 * USDC as u128);
}

// ---- Staker profit share ----

/// A house holding `equity` at its high-water mark, with one active staker.
fn house(equity: u64) -> (State, Pubkey) {
    let mut s = ledger::tests::state();
    let staker = Pubkey::new_unique();
    let i = ensure_user(&mut s, staker).unwrap();
    s.users[i].stake = 1_000;
    s.total_stake = 1_000;
    s.house_cash = equity;
    s.tracked_lp = equity as i128;
    s.lp_highwater = equity;
    (s, Pubkey::new_unique())
}

fn identity(s: &State) {
    assert_eq!(
        s.tracked_lp,
        s.house_cash as i128 - ledger::debt(s).unwrap() as i128
    );
}

/// Settle the trader's position `id` at `price`; return what stakers received
/// beyond their half of the 2% fee.
fn settle_shared(s: &mut State, id: u64, price: u64, forced: bool) -> u64 {
    let (staker_before, team_before) = (s.fee_staker, s.fee_team);
    let vault = expected_vault(s).unwrap();
    let i = s.positions.iter().position(|p| p.id == id).unwrap();
    settle_position(s, i, price, id, forced).unwrap();
    assert_eq!(expected_vault(s).unwrap(), vault);
    identity(s);
    // The fee splits evenly, with an odd atom going to the team, so anything
    // stakers received above the team's amount is profit share.
    let (staker, team) = (s.fee_staker - staker_before, s.fee_team - team_before);
    assert!(staker + 1 >= team);
    staker.saturating_sub(team)
}

#[test]
fn equity_regained_after_a_payout_is_not_shared_again() {
    let (mut s, t) = house(1_000_000 * USDC);
    ensure_user(&mut s, t).unwrap();
    // A winner takes about $860 out of the house.
    s.positions.push(pos(1, t, 1_000 * USDC, 1));
    assert_eq!(settle_shared(&mut s, 1, 101_000_000, false), 0);
    let drawdown = 1_000_000 * USDC as i128 - s.tracked_lp;
    assert!(drawdown > 800 * USDC as i128);
    assert_eq!(s.lp_highwater, 1_000_000 * USDC, "the mark never falls");
    // A $500 loss only refills part of that: nothing is shared.
    s.positions.push(pos(2, t, 500 * USDC, 1));
    assert_eq!(settle_shared(&mut s, 2, 99_000_000, true), 0);
    assert!(s.tracked_lp < 1_000_000 * USDC as i128);
    // The next $1,000 loss crosses the mark. Only the part above it is shared,
    // at the marginal rate of 99%.
    let below = 1_000_000 * USDC as i128 - s.tracked_lp;
    s.positions.push(pos(3, t, 1_000 * USDC, 1));
    let shared = settle_shared(&mut s, 3, 99_000_000, true);
    let above = (980 * USDC as i128 - below) as u64;
    assert!(above > 0);
    let expected = math::lp_profit_share(1_000_000 * USDC as i128, above).unwrap();
    assert_eq!(shared, expected);
    assert!(shared.abs_diff(above * 99 / 100) <= 2);
    assert_eq!(s.lp_highwater as i128, s.tracked_lp);
    assert!(s.lp_highwater > 1_000_000 * USDC);
}

#[test]
fn a_pair_settled_winner_first_shares_only_its_net_gain() {
    // The same owner on both sides of the market: one leg wins what the other
    // loses, less the house edge. With the winner settled first, stakers get
    // a share of that edge only, never of the gross loss.
    let (mut s, t) = house(1_000_000 * USDC);
    ensure_user(&mut s, t).unwrap();
    s.positions.push(pos(1, t, 2_000 * USDC, 1));
    s.positions.push(pos(2, t, 2_000 * USDC, -1));
    let price = 99_100_000; // 0.9% down: the long loses $1,800
    assert_eq!(settle_shared(&mut s, 2, price, false), 0);
    let shared = settle_shared(&mut s, 1, price, false);
    let net = s.tracked_lp + shared as i128 - 1_000_000 * USDC as i128;
    assert!(
        net > 0 && net < 250 * USDC as i128,
        "house edge on the pair"
    );
    assert_eq!(
        shared,
        math::lp_profit_share(1_000_000 * USDC as i128, net as u64).unwrap()
    );
}

#[test]
fn nothing_is_exported_while_nobody_is_staked_and_it_is_not_shared_later() {
    let mut s = ledger::tests::state();
    let t = Pubkey::new_unique();
    ensure_user(&mut s, t).unwrap();
    s.house_cash = 1_000_000 * USDC;
    s.tracked_lp = s.house_cash as i128;
    s.lp_highwater = s.house_cash;
    s.positions.push(pos(1, t, 10_000 * USDC, 1));
    assert_eq!(settle_shared(&mut s, 1, 99_000_000, true), 0);
    // All $9,800 after fees stays in the house and lifts the mark.
    assert_eq!(s.house_cash, 1_009_800 * USDC);
    assert_eq!(s.lp_highwater, 1_009_800 * USDC);
    // Somebody stakes afterwards: only growth from here on is shared.
    let i = ensure_user(&mut s, Pubkey::new_unique()).unwrap();
    s.users[i].stake = 1;
    s.total_stake = 1;
    s.positions.push(pos(2, t, 100 * USDC, 1));
    let shared = settle_shared(&mut s, 2, 99_000_000, true);
    assert_eq!(
        shared,
        math::lp_profit_share(1_009_800 * USDC as i128, 98 * USDC).unwrap()
    );
}

#[test]
fn equity_created_by_destroying_an_iou_raises_the_mark_without_sharing() {
    // A position opened with $100 of IOU while the house also holds the cash
    // that would cover it. Equity sits exactly at the mark.
    let (mut s, t) = house(1_000_000 * USDC);
    ensure_user(&mut s, t).unwrap();
    let mut p = pos(1, t, 100 * USDC, 1);
    p.debt_funded = true;
    s.positions.push(p);
    s.house_cash += 100 * USDC;
    identity(&s);
    // It busts: the IOU is destroyed, lifting equity $100 above the mark. No
    // trader paid that in, so none of it is exported.
    assert_eq!(settle_shared(&mut s, 1, 99_000_000, true), 0);
    assert_eq!(s.fee_staker, 0);
    assert_eq!(s.tracked_lp, 1_000_100 * USDC as i128);
    assert_eq!(s.lp_highwater, 1_000_100 * USDC);
}

#[test]
fn emission_measures_the_flat_region_from_the_share_base() {
    // Equity $100k below a $2.5M mark: the gain is retained in full, so the
    // flat 100-per-dollar region ends after exactly $100k of it.
    let before = 1_900_000 * USDC as i128;
    let mark = 2_500_000 * USDC as i128;
    let gain = 300_000 * USDC;
    let mut tail = 0;
    let minted = math::emission_with_lp_profit_share(gain, gain, before, mark, &mut tail).unwrap();
    assert_eq!(tail, 200_000 * USDC, "tail progress past $2M of equity");
    let mut plain_tail = 0;
    assert_eq!(
        minted,
        math::emission(gain, gain, before, &mut plain_tail).unwrap()
    );
    // With no drawdown the same gain is shared on the way up, so more of it is
    // needed to reach $2M and the flat region lasts longer.
    let mut shared_tail = 0;
    let shared =
        math::emission_with_lp_profit_share(gain, gain, before, before, &mut shared_tail).unwrap();
    assert!(shared_tail < tail && shared > minted);
    // A mark below current equity is the same as no mark.
    let mut low = 0;
    assert_eq!(
        math::emission_with_lp_profit_share(gain, gain, before, 0, &mut low).unwrap(),
        shared
    );
}

/// Settle a run of busted cash positions of the given margins against a staked
/// house, through the real settlement code. Returns (equity, mark, staker
/// fees and share, PENCIL minted, tail progress).
fn run_losses(equity: u64, mark: u64, margins: &[u64]) -> (i128, u64, u64, u64, u64) {
    let (mut s, trader) = house(equity);
    s.lp_highwater = mark;
    ensure_user(&mut s, trader).unwrap();
    let vault = expected_vault(&s).unwrap();
    for (n, &margin) in margins.iter().enumerate() {
        ledger::raise_lp_highwater(&mut s).unwrap();
        s.positions.push(pos(n as u64 + 1, trader, margin, 1));
        settle_position(&mut s, 0, 99_000_000, n as u64 + 1, true).unwrap();
        identity(&s);
        assert!(s.lp_highwater as i128 >= s.tracked_lp);
    }
    assert_eq!(
        expected_vault(&s).unwrap() - vault,
        margins.iter().sum::<u64>()
    );
    (
        s.tracked_lp,
        s.lp_highwater,
        s.fee_staker,
        s.total_token_credits,
        s.tail_highwater,
    )
}

#[test]
fn splitting_a_loss_across_settlements_changes_only_rounding_whatever_the_mark() {
    // Deterministic pseudo-random cases across the bootstrap buffer, both
    // curve regions, the $2M emission boundary and marks above and below equity.
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = |bound: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % bound
    };
    for case in 0..2_000 {
        let equity = next(6_000_000) * USDC + next(USDC);
        let mark = (equity + next(3_000_000) * USDC).saturating_sub(1_000_000 * USDC);
        let margin = 50 + next(5_000_000 * USDC);
        let first = next(margin + 1);
        let whole = run_losses(equity, mark, &[margin]);
        let split = run_losses(equity, mark, &[first, margin - first]);
        let label =
            format!("case {case}: equity {equity}, mark {mark}, margin {margin}, first {first}");
        assert!(whole.0.abs_diff(split.0) <= 8, "equity: {label}");
        assert!(whole.1.abs_diff(split.1) <= 8, "mark: {label}");
        assert!(whole.2.abs_diff(split.2) <= 8, "stakers: {label}");
        assert!(
            whole.4.abs_diff(split.4) <= 900,
            "tail: {label}, whole {}, split {}",
            whole.4,
            split.4
        );
        assert!(whole.3.abs_diff(split.3) <= 90_100, "minted: {label}");
        // Nothing beyond the fee is shared below the mark or the buffer.
        let gain = margin - margin / 50;
        if equity + gain <= mark.max(math::LP_PROFIT_BUFFER as u64) {
            assert_eq!(whole.2, margin / 50 / 2, "{label}");
            assert_eq!(whole.0, (equity + gain) as i128, "{label}");
        }
        // Tail emissions begin exactly when actual equity passes $2M.
        let above = whole.0 - math::FLAT_THRESHOLD;
        if (equity as i128) < math::FLAT_THRESHOLD {
            assert!(
                (whole.4 > 0) == (above > 0) || above.abs() <= 110,
                "boundary: {label}"
            );
        } else {
            assert_eq!(whole.4, gain, "{label}");
        }
    }
}

#[test]
fn settlement_measures_emission_from_the_mark_not_from_opening_equity() {
    // Equity $100k under a $2.5M mark, just below the $2M emission boundary.
    // A $300k bust is retained in full, so equity passes $2M after $100k of
    // its $294k gain and the other $194k is tail progress. Measured from the
    // opening equity instead, the share curve would stretch the flat region
    // over the whole gain.
    let (mut s, trader) = house(1_900_000 * USDC);
    s.lp_highwater = 2_500_000 * USDC;
    ensure_user(&mut s, trader).unwrap();
    s.positions.push(pos(1, trader, 300_000 * USDC, 1));
    assert_eq!(settle_shared(&mut s, 1, 99_000_000, true), 0);
    assert_eq!(s.tracked_lp, 2_194_000 * USDC as i128);
    assert_eq!(s.tail_highwater, 194_000 * USDC);
    let mut tail = 0;
    let expected = math::emission(
        300_000 * USDC,
        294_000 * USDC,
        1_900_000 * USDC as i128,
        &mut tail,
    )
    .unwrap();
    assert_eq!(s.total_token_credits, expected);
}

fn stopped(equity: u64) -> (State, usize) {
    let (mut s, _) = house(equity);
    s.markets = vec![market(0), market(1)];
    for m in &mut s.markets {
        m.halted = true;
    }
    s.paused = true;
    (s, 0)
}

#[test]
fn residual_house_cash_goes_to_stakers_only_at_the_very_end() {
    let (mut s, staker) = stopped(40_000 * USDC);
    let vault = expected_vault(&s).unwrap();
    assert_eq!(
        ledger::sweep_terminal_residual(&mut s).unwrap(),
        40_000 * USDC
    );
    assert_eq!(s.house_cash, 0);
    assert_eq!(s.tracked_lp, 0);
    assert_eq!(s.fee_staker, 40_000 * USDC);
    assert_eq!(expected_vault(&s).unwrap(), vault);
    identity(&s);
    settle_rewards(&mut s, staker).unwrap();
    assert_eq!(s.users[staker].reward_credit, 40_000 * USDC);
    assert_eq!(ledger::sweep_terminal_residual(&mut s).unwrap(), 0);

    let blocked = |change: fn(&mut State)| {
        let (mut s, _) = stopped(40_000 * USDC);
        change(&mut s);
        let swept = ledger::sweep_terminal_residual(&mut s).unwrap();
        assert_eq!(s.house_cash, 40_000 * USDC);
        swept
    };
    // One market still trading.
    assert_eq!(blocked(|s| s.markets[1].halted = false), 0);
    // A cash-funded position still open.
    assert_eq!(
        blocked(|s| {
            let owner = Pubkey::new_unique();
            ensure_user(s, owner).unwrap();
            s.positions.push(pos(1, owner, USDC, 1));
        }),
        0
    );
    // An IOU-funded position still open.
    assert_eq!(
        blocked(|s| {
            let owner = Pubkey::new_unique();
            ensure_user(s, owner).unwrap();
            let mut p = pos(1, owner, USDC, 1);
            p.debt_funded = true;
            s.positions.push(p);
        }),
        0
    );
    // A payout still waiting, here in a record this transaction did not load.
    assert_eq!(
        blocked(|s| {
            s.totals.claims = 1;
            s.next_claim = 2;
        }),
        0
    );
    // An IOU reserved by an intent that has not been cleared yet.
    assert_eq!(blocked(|s| s.totals.pending_debt = 1), 0);
    // Nobody staked: the cash waits for a staker instead of going unassigned.
    assert_eq!(
        blocked(|s| {
            s.users[0].stake = 0;
            s.total_stake = 0;
        }),
        0
    );
    // No markets at all is not "every market stopped".
    assert_eq!(blocked(|s| s.markets.clear()), 0);
}

#[test]
fn harvest_pays_the_residual_once_everything_is_stopped_and_settled() {
    let (s, _) = stopped(40_000 * USDC);
    let mut root = s.clone();
    root.users.clear();
    let after = enter(&root, Instruction::Harvest { max: 4 }, Call::default()).unwrap();
    assert_eq!(after.house_cash, 0);
    assert_eq!(after.tracked_lp, 0);
    assert_eq!(after.fee_staker, 40_000 * USDC);
    assert!(after.reward_index > 0);
    // With a market still trading the same call moves nothing.
    root.markets[1].halted = false;
    root.paused = false;
    let after = enter(&root, Instruction::Harvest { max: 4 }, Call::default()).unwrap();
    assert_eq!(after.house_cash, 40_000 * USDC);
    assert_eq!(after.fee_staker, 0);
}

#[test]
fn reward_index_keeps_small_distributions_over_a_large_stake() {
    // 500 billion PENCIL staked; one USDC atom distributed at a time. At the
    // old 1e12 scale each of these rounded to nothing and was lost.
    let mut s = ledger::tests::state();
    let i = ensure_user(&mut s, Pubkey::new_unique()).unwrap();
    let stake = 500_000_000_000 * 1_000_000u64;
    s.users[i].stake = stake;
    s.total_stake = stake;
    for _ in 0..1_000 {
        ledger::distribute(&mut s, 1).unwrap();
    }
    settle_rewards(&mut s, i).unwrap();
    assert_eq!(s.users[i].reward_credit, 1_000);
    // The largest amounts the ledger can hold still fit the index arithmetic.
    let mut s = ledger::tests::state();
    let i = ensure_user(&mut s, Pubkey::new_unique()).unwrap();
    s.users[i].stake = 1;
    s.total_stake = 1;
    ledger::distribute(&mut s, u64::MAX).unwrap();
    settle_rewards(&mut s, i).unwrap();
    assert_eq!(s.users[i].reward_credit, u64::MAX);
    let mut s = ledger::tests::state();
    let i = ensure_user(&mut s, Pubkey::new_unique()).unwrap();
    s.users[i].stake = u64::MAX;
    s.total_stake = u64::MAX;
    ledger::distribute(&mut s, u64::MAX).unwrap();
    settle_rewards(&mut s, i).unwrap();
    assert_eq!(s.users[i].reward_credit, u64::MAX);
}

#[test]
fn staking_constants_are_the_reviewed_values() {
    assert_eq!(REWARD_SCALE, 10u128.pow(18));
    assert_eq!(math::LP_PROFIT_BUFFER, 5_000 * USDC as i128);
}

// ---- Settlement order ----

/// One owner holds `pairs` long/short pairs of `margin` each at `leverage`;
/// the mark then moves by `move_bps`. Settle every leg of one side, then
/// the other. Returns (change in LP equity, staker income, team income).
fn settle_pairs(
    equity: u64,
    pairs: u64,
    margin: u64,
    leverage: u16,
    move_bps: u64,
    losers_first: bool,
    move_up: bool,
) -> (i128, u64, u64) {
    let (mut s, trader) = house(equity);
    ensure_user(&mut s, trader).unwrap();
    for n in 0..pairs {
        for side in [1i8, -1] {
            let mut p = pos(1 + 2 * n + (side < 0) as u64, trader, margin, side);
            p.leverage = leverage;
            s.positions.push(p);
        }
    }
    let change = 100_000_000 * move_bps / 10_000;
    let price = if move_up {
        100_000_000 + change
    } else {
        100_000_000 - change
    };
    let loser = if move_up { -1 } else { 1 };
    let sides: [i8; 2] = if losers_first {
        [loser, -loser]
    } else {
        [-loser, loser]
    };
    let vault = expected_vault(&s).unwrap();
    for side in sides {
        while let Some(i) = s.positions.iter().position(|p| p.side == side) {
            let p = s.positions[i].clone();
            let bust = math::busted(p.entry, price, p.side, p.leverage);
            settle_position(&mut s, i, price, p.id, bust).unwrap();
            identity(&s);
        }
    }
    assert_eq!(expected_vault(&s).unwrap(), vault);
    (s.tracked_lp - equity as i128, s.fee_staker, s.fee_team)
}
#[test]
fn no_order_of_settling_both_sides_of_a_market_takes_equity_out_of_the_house() {
    // At 99%, distributing gross losing-leg proceeds would pay a staker
    // more than the pair costs. The exposure reserve defers that allocation.
    // Test both signs, both close orders, and one or many split positions.
    for equity in [
        0u64, 4_000, 60_000, 250_000, 1_000_000, 3_000_000, 50_000_000,
    ] {
        for (leverage, move_bps) in [
            (200u16, 30u64),
            (100, 3),
            (100, 10),
            (100, 30),
            (100, 90),
            (20, 400),
            (5, 1_500),
            (1, 5_000),
        ] {
            for pairs in [1u64, 40, 400] {
                let margin = 250_000 * USDC / leverage as u64;
                let label =
                    format!("equity ${equity}, {pairs} pairs at {leverage}x, {move_bps} bps");
                let equity = equity * USDC;
                let loss = pairs * margin * leverage as u64 * move_bps / 10_000;
                for move_up in [false, true] {
                    for losers_first in [true, false] {
                        let (change, stakers, team) = settle_pairs(
                            equity,
                            pairs,
                            margin,
                            leverage,
                            move_bps,
                            losers_first,
                            move_up,
                        );
                        assert!(
                            change >= 0,
                            "{label}, move up {move_up}, losers first {losers_first}: {change}"
                        );
                        // A coalition owning all active stake still pays positive net USDC.
                        let exit = if move_up {
                            100_000_000 + 100_000_000 * move_bps / 10_000
                        } else {
                            100_000_000 - 100_000_000 * move_bps / 10_000
                        };
                        let net_win = math::pnl(
                            margin,
                            leverage,
                            100_000_000,
                            exit,
                            if move_up { 1 } else { -1 },
                            814598,
                        )
                        .unwrap()
                        .1 * pairs;
                        let trader_cost = loss - net_win;
                        assert!(stakers < trader_cost, "{label}, move up {move_up}, losers first {losers_first}: stakers {stakers}, cost {trader_cost}");
                        assert_eq!(change + stakers as i128 + team as i128, trader_cost as i128);
                    }
                }
            }
        }
    }
}

// ---- Settling a stopped market ----

/// Stop the only market at a mark 1% below entry and settle $1M of busted
/// longs against one $1M short, in the given order. Returns what stakers and
/// the team received and what the winner is still owed at the end.
fn wind_down_in_order(losers_first: bool) -> (u64, u64, u64) {
    let (mut s, trader) = house(50_000 * USDC);
    let winner = Pubkey::new_unique();
    ensure_user(&mut s, trader).unwrap();
    ensure_user(&mut s, winner).unwrap();
    s.markets = vec![market(0)];
    s.markets[0].last_mark = 99_000_000;
    s.markets[0].last_mark_slot = 5;
    s.markets[0].last_mark_at = 5;
    s.markets[0].halted = true;
    s.paused = true;
    for id in 1..=4 {
        s.positions.push(pos(id, trader, 250_000 * USDC, 1));
    }
    s.positions.push(pos(5, winner, 1_000_000 * USDC, -1));
    let vault = expected_vault(&s).unwrap();
    let order: [u64; 5] = if losers_first {
        [1, 2, 3, 4, 5]
    } else {
        [5, 1, 2, 3, 4]
    };
    for id in order {
        settle_wind_down(&mut s, id).unwrap();
        identity(&s);
    }
    ledger::harvest(&mut s, 16).unwrap();
    ledger::sweep_terminal_residual(&mut s).unwrap();
    identity(&s);
    assert_eq!(expected_vault(&s).unwrap(), vault);
    let unpaid: u64 = s.claims.iter().map(|c| c.amount).sum();
    (s.fee_staker, s.fee_team, unpaid)
}

#[test]
fn wind_down_settlement_order_cannot_leave_a_winner_unpaid() {
    // Anyone may settle any position of a stopped market, so a staker could
    // settle the losers first. With fees and profit share taken along the way
    // that left the winner about $122k short, with no later loss to repay it.
    let (losers_first, winner_first) = (wind_down_in_order(true), wind_down_in_order(false));
    assert_eq!(losers_first.2, 0, "the winner is paid in full");
    assert_eq!(losers_first, winner_first, "the order changes nothing");
    // No fee was taken; stakers received exactly the residual, at the end.
    let (stakers, team, _) = losers_first;
    assert_eq!(team, 0);
    assert!(stakers > 185_000 * USDC && stakers < 187_000 * USDC);
}
