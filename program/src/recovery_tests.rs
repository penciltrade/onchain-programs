use crate::{
    ledger::{self, ensure_user, expected_vault},
    types::*,
    *,
};
use solana_program::program_error::ProgramError;

/// First slot at which a market last checkpointed at slot 20 may wind down.
const LATE: u64 = 20 + ORACLE_MIN_SLOTS;

/// Syscall stubs are process-wide and tests run on several threads. Every test
/// that installs stubs holds this lock for as long as they are installed.
pub(crate) static SYSCALLS: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Execute the real native entrypoint with an injected chain Clock. Production
/// contains no accelerated timeout flag or alternate recovery constants.
fn timed_entrypoint(s: State, ix: Instruction, now: i64, slot: u64) -> Result<State, ProgramError> {
    timed_entrypoint_by(s, ix, now, slot, false)
}
/// As above; `guardian` makes the signing actor the deployment's guardian.
fn timed_entrypoint_by(
    mut s: State,
    ix: Instruction,
    now: i64,
    slot: u64,
    guardian: bool,
) -> Result<State, ProgramError> {
    use solana_program::{
        program_option::COption,
        program_stubs::{set_syscall_stubs, SyscallStubs},
    };
    use spl_token::state::AccountState;
    // The syscall stubs are process-wide and tests run in parallel: one at a time.
    static CLOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _turn = CLOCK.lock().unwrap_or_else(|e| e.into_inner());
    struct Stubs(Clock);
    impl SyscallStubs for Stubs {
        fn sol_get_clock_sysvar(&self, address: *mut u8) -> u64 {
            unsafe {
                address.cast::<Clock>().write(self.0.clone());
            }
            0
        }
        fn sol_get_rent_sysvar(&self, address: *mut u8) -> u64 {
            unsafe {
                address.cast::<Rent>().write(Rent::default());
            }
            0
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
    let _lock = SYSCALLS.lock().unwrap_or_else(|e| e.into_inner());
    let _restore = Restore(Some(set_syscall_stubs(Box::new(Stubs(Clock {
        slot,
        unix_timestamp: now,
        ..Clock::default()
    })))));
    let id = Pubkey::new_unique();
    let (root, bump) = Pubkey::find_program_address(&[b"state"], &id);
    s.bump = bump;
    let actor = Pubkey::new_unique();
    if guardian {
        s.guardian = actor;
    }
    let system = solana_program::system_program::id();
    let token = spl_token::id();
    let keys = [
        root,
        actor,
        s.usdc_vault,
        s.pencil_mint,
        s.stake_vault,
        token,
        Pubkey::new_unique(),
        Pubkey::new_unique(),
        s.sequencer,
        s.oracle_program,
        system,
        actor,
    ];
    let owners = [
        id, system, token, token, token, system, system, system, system, system, system, system,
    ];
    let mut data: Vec<Vec<u8>> = (0..12).map(|_| Vec::new()).collect();
    let bytes = borsh::to_vec(&s).unwrap();
    data[0] = vec![0; 8192];
    data[0][..4].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
    data[0][4..4 + bytes.len()].copy_from_slice(&bytes);
    let vault = |mint| TokenAccount {
        mint,
        owner: root,
        amount: 0,
        delegate: COption::None,
        state: AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };
    data[2] = vec![0; TokenAccount::LEN];
    TokenAccount::pack(vault(s.usdc_mint), &mut data[2]).unwrap();
    data[4] = vec![0; TokenAccount::LEN];
    TokenAccount::pack(vault(s.pencil_mint), &mut data[4]).unwrap();
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
    let mut lamports = [1_000_000_000; 12];
    let accounts: Vec<AccountInfo> = keys
        .iter()
        .zip(owners.iter())
        .zip(lamports.iter_mut())
        .zip(data.iter_mut())
        .enumerate()
        .map(|(n, (((key, owner), balance), bytes))| {
            AccountInfo::new(
                key,
                n == 1 || n == 11,
                true,
                balance,
                bytes,
                owner,
                n == 5 || n == 10,
                0,
            )
        })
        .collect();
    process_instruction(&id, &accounts, &borsh::to_vec(&ix).unwrap())?;
    read_state(&accounts[0])
}

#[test]
fn native_entrypoint_enforces_recovery_boundaries() {
    let s = fixture();
    assert_eq!(
        timed_entrypoint(
            s.clone(),
            Instruction::RemoveRelayer,
            100 + SEQUENCER_TIMEOUT - 1,
            1
        )
        .unwrap_err(),
        ProgramError::Custom(RECOVERY_DELAY)
    );
    let public = timed_entrypoint(
        s,
        Instruction::RemoveRelayer,
        100 + SEQUENCER_TIMEOUT,
        1,
    )
    .unwrap();
    assert!(public.permissionless);
    // The guardian must meet the same inactivity threshold as everyone else.
    let early = |guardian| {
        timed_entrypoint_by(
            fixture(),
            Instruction::RemoveRelayer,
            101,
            1,
            guardian,
        )
    };
    assert_eq!(early(true).unwrap_err(), ProgramError::Custom(RECOVERY_DELAY));
    assert_eq!(
        early(false).unwrap_err(),
        ProgramError::Custom(RECOVERY_DELAY)
    );
    let mut s = fixture();
    checkpoint(&mut s, 0, 100, 20, 101).unwrap();
    let mut second = s.markets[0].clone();
    second.id = 1;
    second.last_mark_at = 101 + ORACLE_TIMEOUT;
    s.markets.push(second);
    assert_eq!(
        timed_entrypoint(
            s.clone(),
            Instruction::WindDownMarket { market: 0 },
            101 + ORACLE_TIMEOUT - 1,
            LATE
        )
        .unwrap_err(),
        ProgramError::Custom(RECOVERY_DELAY)
    );
    let stopped = timed_entrypoint(
        s,
        Instruction::WindDownMarket { market: 0 },
        101 + ORACLE_TIMEOUT,
        LATE,
    )
    .unwrap();
    assert!(stopped.markets[0].halted);
    assert!(!stopped.markets[1].halted);
    assert!(!stopped.paused);
}

#[test]
fn v4_root_has_a_cross_language_wire_fixture() {
    let mut s = fixture();
    let key = |n| Pubkey::new_from_array([n; 32]);
    s.usdc_mint = key(1);
    s.pencil_mint = key(2);
    s.usdc_vault = key(3);
    s.stake_vault = key(4);
    s.sequencer = key(5);
    s.oracle_program = key(6);
    s.guardian = key(7);
    s.team = key(8);
    s.admins = [key(9), key(10), key(11)];
    s.markets[0].oracle_accounts = vec![key(12)];
    checkpoint(&mut s, 0, 80_000_000_000, 100, 150).unwrap();
    s.outcomes.push(Outcome {
        ticket: 7,
        code: 4,
        position_id: 9,
        price: 123,
        pnl: -456,
        minted_credit: 789,
    });
    let body = borsh::to_vec(&s).unwrap();
    let mut data = (body.len() as u32).to_le_bytes().to_vec();
    data.extend(body);
    let hex = data.iter().map(|b| format!("{b:02x}")).collect::<String>() + "\n";
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../tests/fixtures/v4-state.hex"
    );
    if std::env::var_os("PENCIL_UPDATE_WIRE_FIXTURE").is_some() {
        std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap()).unwrap();
        std::fs::write(path, &hex).unwrap();
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), hex);
}

fn fixture() -> State {
    let mut s = ledger::tests::state();
    s.started_at = 100;
    s.last_sequenced_at = 100;
    s.markets.push(Market {
        id: 0,
        symbol: *b"BTC\0\0\0\0\0",
        oracle_accounts: vec![Pubkey::new_unique()],
        tick_price_micro: 1,
        position_multiplier_milli: 814598,
        max_spread_bps: 0,
        max_deviation_bps: 50,
        max_age_slots: 20,
        next_seq: 1,
        version: 1,
        fill_delay_slots: 0,
        fill_window_slots: 1,
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
    });
    s
}
fn position(s: &mut State, owner: Pubkey, debt: bool) -> u64 {
    ensure_user(s, owner).unwrap();
    let id = s.next_position;
    s.next_position += 1;
    s.positions.push(Position {
        id,
        owner,
        market: 0,
        side: 1,
        margin: 100_000_000,
        leverage: 200,
        entry: 80_000_000_000,
        debt_funded: debt,
        opened_at: 100,
        config_version: 1,
        position_multiplier_milli: 814598,
        close_pending: false,
        exit_ready_at: 0,
        exit_limit_price: 0,
    });
    if debt {
        s.tracked_lp -= 100_000_000;
    }
    id
}
#[test]
fn timeout_cannot_enable_unsealed_or_early_admission_and_never_recloses() {
    let mut s = fixture();
    s.immutable = false;
    assert!(remove_relayer(&mut s, 1000).is_err());
    assert!(remove_relayer(&mut s, 1000).is_err());
    s.immutable = true;
    assert_eq!(
        remove_relayer(&mut s, 100 + SEQUENCER_TIMEOUT - 1),
        Err(ProgramError::Custom(RECOVERY_DELAY))
    );
    assert!(!s.permissionless);
    remove_relayer(&mut s, 100 + SEQUENCER_TIMEOUT).unwrap();
    s.last_sequenced_at = 170;
    remove_relayer(&mut s, 170).unwrap();
    assert!(s.permissionless);
}
#[test]
fn relayer_removal_is_explicit_and_has_no_guardian_bypass() {
    let mut s = fixture();
    s.immutable = false;
    assert_eq!(
        remove_relayer(&mut s, 101),
        Err(ProgramError::Custom(NOT_SEALED))
    );
    s.immutable = true;
    assert_eq!(remove_relayer(&mut s, 101), Err(ProgramError::Custom(RECOVERY_DELAY)));
    remove_relayer(&mut s, 100 + SEQUENCER_TIMEOUT).unwrap();
    assert!(s.permissionless);
    // One-way: a later call by anyone leaves it open.
    remove_relayer(&mut s, 102).unwrap();
    assert!(s.permissionless);
}
#[test]
fn guardian_halt_authority_ends_only_after_explicit_relayer_removal() {
    let mut s = fixture();
    s.local_mode = false;
    let guarded = timed_entrypoint_by(s.clone(), Instruction::Pause, 101, 1, true).unwrap();
    assert!(guarded.paused && guarded.markets.iter().all(|m| m.halted));
    let late = 100 + SEQUENCER_TIMEOUT;
    remove_relayer(&mut s, late).unwrap();
    assert!(!s.paused && s.markets.iter().all(|m| !m.halted));
    for guardian in [false, true] {
        assert_eq!(
            timed_entrypoint_by(s.clone(), Instruction::Pause, late + 1, 2, guardian).unwrap_err(),
            ProgramError::Custom(IMMUTABLE)
        );
    }
    assert!(s.permissionless && s.immutable);
}
#[test]
fn seal_cannot_freeze_a_paused_or_halted_trial() {
    for halted in [false, true] {
        let mut s = fixture();
        s.immutable = false;
        s.paused = !halted;
        s.markets[0].halted = halted;
        assert_eq!(
            timed_entrypoint(s, Instruction::Seal, 101, 1).unwrap_err(),
            ProgramError::Custom(PAUSED)
        );
    }
}
#[test]
fn only_advancing_checkpoints_renew_liveness() {
    let mut s = fixture();
    checkpoint(&mut s, 0, 100, 20, 101).unwrap();
    checkpoint(&mut s, 0, 100, 20, 300).unwrap();
    assert_eq!(s.markets[0].last_mark_at, 101);
    assert!(checkpoint(&mut s, 0, 101, 20, 301).is_err());
    assert!(checkpoint(&mut s, 0, 100, 19, 301).is_err());
    checkpoint(&mut s, 0, 101, 21, 302).unwrap();
    assert_eq!(s.markets[0].last_mark_at, 302);
}
#[test]
fn stale_wind_down_needs_elapsed_time_and_elapsed_slots() {
    let mut s = fixture();
    checkpoint(&mut s, 0, 100, 20, 101).unwrap();
    assert!(wind_down_market(&mut s, 0, 100 + ORACLE_TIMEOUT, LATE).is_err());
    // The quote's own max age (20 slots here) no longer opens the deadline,
    // and neither does any amount of wall-clock time on its own.
    assert!(wind_down_market(&mut s, 0, 101 + ORACLE_TIMEOUT, 41).is_err());
    assert!(wind_down_market(&mut s, 0, 101 + 10 * ORACLE_TIMEOUT, LATE - 1).is_err());
    wind_down_market(&mut s, 0, 101 + ORACLE_TIMEOUT, LATE).unwrap();
    assert!(s.markets[0].halted && s.paused);
    assert!(checkpoint(&mut s, 0, 999, LATE, 402).is_err());
    wind_down_market(&mut s, 0, 102 + ORACLE_TIMEOUT, LATE).unwrap();
    assert_eq!(s.markets[0].last_mark, 100);
}
#[test]
fn never_priced_market_can_stop_but_cannot_invent_a_settlement_price() {
    let mut s = fixture();
    assert!(wind_down_market(&mut s, 0, 99 + ORACLE_TIMEOUT, 1000).is_err());
    wind_down_market(&mut s, 0, 100 + ORACLE_TIMEOUT, 1000).unwrap();
    let id = position(&mut s, Pubkey::new_unique(), false);
    assert!(settle_wind_down(&mut s, id).is_err());
}
#[test]
fn stopping_one_market_does_not_stop_another() {
    let mut s = fixture();
    let mut second = s.markets[0].clone();
    second.id = 1;
    s.markets.push(second);
    wind_down_market(&mut s, 0, 100 + ORACLE_TIMEOUT, 1000).unwrap();
    assert!(!s.paused && !s.markets[1].halted);
    checkpoint(&mut s, 1, 99, 1000, 401).unwrap();
}
#[test]
fn frozen_settlement_preserves_margin_and_places_unfunded_profit_in_fifo() {
    let mut s = fixture();
    let owner = Pubkey::new_unique();
    let id = position(&mut s, owner, false);
    checkpoint(&mut s, 0, 80_800_000_000, 20, 101).unwrap();
    assert!(settle_wind_down(&mut s, id).is_err());
    wind_down_market(&mut s, 0, 101 + ORACLE_TIMEOUT, LATE).unwrap();
    let before = expected_vault(&s).unwrap();
    let o = settle_wind_down(&mut s, id).unwrap();
    assert_eq!(o.price, 80_800_000_000);
    assert_eq!(s.users[0].available, 100_000_000);
    assert_eq!(s.claims.len(), 1);
    assert!(s.claims[0].amount > 0);
    assert_eq!(s.tracked_lp, -(s.claims[0].amount as i128));
    assert_eq!(expected_vault(&s).unwrap(), before);
    assert!(settle_wind_down(&mut s, id).is_err());
}
#[test]
fn frozen_bust_and_debt_settlement_reconcile_cash_debt_and_emission() {
    for debt in [false, true] {
        let mut s = fixture();
        let owner = Pubkey::new_unique();
        let id = position(&mut s, owner, debt);
        checkpoint(&mut s, 0, 79_500_000_000, 20, 101).unwrap();
        wind_down_market(&mut s, 0, 101 + ORACLE_TIMEOUT, LATE).unwrap();
        let before = expected_vault(&s).unwrap();
        let o = settle_wind_down(&mut s, id).unwrap();
        assert_eq!(o.code, 2);
        assert!(s.positions.is_empty());
        assert_eq!(expected_vault(&s).unwrap(), before);
        assert_eq!(
            s.cash_loss_credits + s.debt_loss_credits,
            s.total_token_credits
        );
        assert_eq!(
            s.tracked_lp,
            s.house_cash as i128 - ledger::debt(&s).unwrap() as i128
        );
    }
}
fn intent(s: &mut State, owner: Pubkey, claim_id: u64) {
    ensure_user(s, owner).unwrap();
    s.intents.push(Intent {
        ticket: 2,
        market_seq: 1,
        owner,
        signer: owner,
        delegate_epoch: 0,
        nonce: 0,
        market: 0,
        kind: 0,
        side: 1,
        margin: 10_000_000,
        leverage: 10,
        claim_id,
        position_id: 0,
        expiry: 200,
        limit_price: 0,
        claim_priority: 1,
        admitted_slot: 20,
    });
    s.next_ticket = 3;
}
#[test]
fn wind_down_cancel_refunds_once_without_advancing_or_deleting_the_ticket() {
    let mut s = fixture();
    let owner = Pubkey::new_unique();
    intent(&mut s, owner, 0);
    s.users[0].pending_cash = 10_000_000;
    assert!(cancel_wind_down_intent(&mut s, 2).is_err());
    wind_down_market(&mut s, 0, 100 + ORACLE_TIMEOUT, 1000).unwrap();
    let before = expected_vault(&s).unwrap();
    cancel_wind_down_intent(&mut s, 2).unwrap();
    cancel_wind_down_intent(&mut s, 2).unwrap();
    assert_eq!(s.users[0].available, 10_000_000);
    assert_eq!(s.users[0].pending_cash, 0);
    assert_eq!(s.execute_ticket, 1);
    assert_eq!(s.intents[0].kind, 2);
    assert_eq!(s.intents[0].ticket, 2);
    assert_eq!(expected_vault(&s).unwrap(), before);
}
#[test]
fn debt_reservation_returns_to_the_original_fifo_claim() {
    let mut s = fixture();
    let owner = Pubkey::new_unique();
    intent(&mut s, owner, 1);
    s.claims.push(Claim {
        id: 1,
        owner,
        priority: 1,
        amount: 5_000_000,
        reserved: 10_000_000,
    });
    s.next_claim = 2;
    s.tracked_lp = -15_000_000;
    wind_down_market(&mut s, 0, 100 + ORACLE_TIMEOUT, 1000).unwrap();
    cancel_wind_down_intent(&mut s, 2).unwrap();
    assert_eq!(s.claims[0].amount, 15_000_000);
    assert_eq!(s.claims[0].reserved, 0);
    assert_eq!(ledger::debt(&s).unwrap(), 15_000_000);
    assert_eq!(s.claim_head, 1);
    assert_eq!(s.claims[0].priority, 1);
}
#[test]
fn legacy_governance_tags_are_rejected_before_accounts_or_privileges_are_loaded() {
    let ixs = [
        Instruction::ProposeUpgrade {
            buffer: Pubkey::new_unique(),
        },
        Instruction::ApplyUpgrade { proposal_id: 1 },
        Instruction::Approve { proposal_id: 1 },
        Instruction::Apply { proposal_id: 1 },
        Instruction::ProposeRotation {
            sequencer: None,
            guardian: None,
            team: Some(Pubkey::new_unique()),
            admin: None,
        },
        Instruction::ApplyRotation { proposal_id: 1 },
        Instruction::SetGovernanceDelay { seconds: 1 },
    ];
    for ix in ixs {
        assert_eq!(
            process_instruction(&Pubkey::new_unique(), &[], &borsh::to_vec(&ix).unwrap()),
            Err(ProgramError::Custom(IMMUTABLE))
        );
    }
}

#[test]
fn unsealed_program_runs_but_nothing_permanent_can_trigger() {
    let mut s = fixture();
    s.immutable = false;
    checkpoint(&mut s, 0, 100, 20, 101).unwrap();
    let long_after = 101 + ORACLE_TIMEOUT + SEQUENCER_TIMEOUT;
    let long_after_slot = 20 + ORACLE_MIN_SLOTS;
    // Every timer has run out, and still no market halts and admission stays sequenced.
    for ix in [
        Instruction::RemoveRelayer,
        Instruction::WindDownMarket { market: 0 },
        Instruction::SettleWindDown { position_id: 1 },
        Instruction::CancelWindDownIntent { ticket: 1 },
    ] {
        assert_eq!(
            timed_entrypoint(s.clone(), ix, long_after, 100_000).unwrap_err(),
            ProgramError::Custom(NOT_SEALED)
        );
    }
    // Ordinary instructions are no longer refused for want of a seal.
    let after = timed_entrypoint(
        s.clone(),
        Instruction::CheckpointOracle { market: 9 },
        long_after,
        100_000,
    );
    assert_ne!(after.unwrap_err(), ProgramError::Custom(NOT_SEALED));
    // The same request once sealed does halt the market.
    s.immutable = true;
    let sealed = timed_entrypoint(
        s,
        Instruction::WindDownMarket { market: 0 },
        long_after,
        long_after_slot,
    )
    .unwrap();
    assert!(sealed.markets[0].halted);
}

#[test]
fn market_configuration_ends_at_the_seal_and_needs_the_upgrade_authority() {
    let s = fixture();
    let market = s.markets[0].clone();
    for ix in [
        Instruction::ConfigureMarket {
            market: market.clone(),
            clear_pause: false,
        },
        Instruction::AddMarket {
            market: market.clone(),
        },
    ] {
        // Sealed: refused outright.
        assert_eq!(
            timed_entrypoint(s.clone(), ix.clone(), 200, 10).unwrap_err(),
            ProgramError::Custom(IMMUTABLE)
        );
        // Unsealed, but without the loader accounts that prove who may upgrade.
        let mut open = s.clone();
        open.immutable = false;
        assert_eq!(
            timed_entrypoint(open, ix, 200, 10).unwrap_err(),
            ProgramError::NotEnoughAccountKeys
        );
    }
}
