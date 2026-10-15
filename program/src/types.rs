use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::pubkey::Pubkey;
pub const STATE_SIZE: usize = 65536;
pub const MAX_BATCH: usize = 16;
pub const MAX_INTENT_TTL: i64 = 30;
pub const EXIT_DELAY: i64 = 15;
/// After 24 hours without sequencing, anyone can explicitly remove the relayer.
/// The transaction permanently removes sequencing and guardian halt authority.
pub const SEQUENCER_TIMEOUT: i64 = 86_400;
/// A full week without advancing checkpoints permits permanent wind-down.
/// Community callers can resume checkpoints during the inactivity grace period.
pub const ORACLE_TIMEOUT: i64 = 604_800;
/// Slots that must also have elapsed since the last checkpoint. The chain
/// clock keeps running through a halt of the chain itself; slots do not, so a
/// restart cannot by itself satisfy the wind-down deadline.
pub const ORACLE_MIN_SLOTS: u64 = 756_000;
/// Largest accepted move between consecutive checkpoints of one market. A
/// change of price scale upstream fails this band instead of pricing trades.
pub const MAX_MARK_MOVE_BPS: u128 = 3_000;
/// A market admits intents only while its last checkpoint is this recent, so
/// an unpriceable market cannot keep feeding the shared FIFO.
pub const ADMISSION_MARK_MAX_AGE: i64 = 120;
/// Configured multiplier bound across representable prices.
pub const MAX_POSITION_MULTIPLIER_MILLI: u64 = 1_000_000_000;
/// Largest single-position notional (margin x leverage), in USDC atoms.
pub const MAX_NOTIONAL: u128 = 10_000_000_000_000;
pub const STAKE_WARMUP: i64 = 3600;
/// Floor on the delayed-governance window outside local mode (24 hours).
pub const MIN_GOVERNANCE_DELAY: i64 = 86_400;
/// Devnet keeps real timing semantics but short enough to iterate on.
pub const DEVNET_MIN_GOVERNANCE_DELAY: i64 = 3;
pub const DEVNET_STAKE_WARMUP: i64 = 300;
/// Upper bounds keep a fill window inside the 30-second intent lifetime.
pub const MAX_FILL_DELAY_SLOTS: u16 = 25;
pub const MAX_FILL_WINDOW_SLOTS: u16 = 50;
pub const REWARD_SCALE: u128 = 1_000_000_000_000_000_000;
#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct Market {
    pub id: u16,
    pub symbol: [u8; 8],
    pub oracle_accounts: Vec<Pubkey>,
    pub tick_price_micro: u64,
    pub position_multiplier_milli: u64,
    pub max_spread_bps: u16,
    pub max_deviation_bps: u16,
    pub max_age_slots: u64,
    pub next_seq: u64,
    pub version: u64,
    /// Slots after admission before the mark may fill an intent (0 = immediate,
    /// local fixtures only). Execution prices only marks updated at or after
    /// admitted_slot + fill_delay_slots and no later than fill_window_slots after.
    pub fill_delay_slots: u16,
    pub fill_window_slots: u16,
    /// Newest mark update slot this program has consumed; marks never go backwards.
    pub last_mark_slot: u64,
    pub last_mark: u64,
    pub last_mark_at: i64,
    pub halted: bool,
    /// Global exposure, including position records omitted by this caller.
    pub long_units: u128,
    pub short_notional: u128,
    /// First committed oracle observation in an execution slot. The source
    /// slots remain separate and still determine freshness and fill windows.
    pub quote_read_slot: u64,
    pub quote_version: u64,
    pub quote_mark: u64,
    pub quote_mark_slot: u64,
    pub quote_index_slot: u64,
    /// 0 empty, 1 guarded quote, 2 rejected quote, 3 same-source-slot conflict.
    pub quote_status: u8,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Exposure {
    pub long_units: u128,
    pub short_notional: u128,
}
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct User {
    pub owner: Pubkey,
    pub available: u64,
    pub pending_cash: u64,
    pub token_credit: u64,
    pub stake: u64,
    pub pending_stake: u64,
    pub stake_ready_at: i64,
    pub reward_index: u128,
    pub reward_credit: u64,
    pub nonce: u64,
    pub delegate: Pubkey,
    pub delegate_expiry: i64,
    pub delegate_budget: u64,
    pub delegate_max_leverage: u16,
    pub delegate_market_mask: u64,
    pub delegate_epoch: u64,
}
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Position {
    pub id: u64,
    pub owner: Pubkey,
    pub market: u16,
    pub side: i8,
    pub margin: u64,
    pub leverage: u16,
    pub entry: u64,
    pub debt_funded: bool,
    pub opened_at: i64,
    pub config_version: u64,
    pub position_multiplier_milli: u64,
    pub close_pending: bool,
    pub exit_ready_at: i64,
    pub exit_limit_price: u64,
}
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Claim {
    pub id: u64,
    pub owner: Pubkey,
    pub amount: u64,
    pub priority: u64,
    pub reserved: u64,
}
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Intent {
    pub ticket: u64,
    pub market_seq: u64,
    pub owner: Pubkey,
    pub signer: Pubkey,
    pub delegate_epoch: u64,
    pub nonce: u64,
    pub market: u16,
    pub kind: u8,
    pub side: i8,
    pub margin: u64,
    pub leverage: u16,
    pub claim_id: u64,
    pub position_id: u64,
    pub expiry: i64,
    pub limit_price: u64,
    pub claim_priority: u64,
    pub admitted_slot: u64,
}
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Outcome {
    pub ticket: u64,
    pub code: u8,
    pub position_id: u64,
    pub price: u64,
    pub pnl: i64,
    pub minted_credit: u64,
}
/// Canonical transaction events. They supplement the bounded recent ring.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum ChainEvent {
    Admission(Intent),
    Outcome {
        outcome: Outcome,
        position: Option<Position>,
    },
}
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Proposal {
    pub id: u64,
    pub execute_after: i64,
    pub base_version: u64,
    pub approvals: u8,
    pub market: u16,
    pub max_spread_bps: u16,
    pub max_deviation_bps: u16,
    pub max_age_slots: u64,
    pub max_leverage: u16,
    pub position_multiplier_milli: u64,
    pub clear_pause: bool,
    pub upgrade_buffer: Option<Pubkey>,
    /// Admin set this proposal was created under; any admin rotation voids it.
    pub admin_epoch: u64,
    pub rotation: Option<Rotation>,
    pub fill_delay_slots: u16,
    pub fill_window_slots: u16,
}
/// Delayed 2-of-3 replacement of operational keys. `None` leaves a key unchanged.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Rotation {
    pub sequencer: Option<Pubkey>,
    pub guardian: Option<Pubkey>,
    pub team: Option<Pubkey>,
    pub admin: Option<(u8, Pubkey)>,
}
#[derive(Clone, Copy, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct Totals {
    pub user_cash: u64,
    pub cash_margin: u64,
    pub claims: u64,
    pub pending_debt: u64,
    pub open_debt: u64,
}
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct State {
    pub version: u8,
    pub bump: u8,
    pub local_mode: bool,
    /// Public test network: pinned stub oracle with real account-owner checks.
    pub devnet: bool,
    pub usdc_mint: Pubkey,
    pub pencil_mint: Pubkey,
    pub usdc_vault: Pubkey,
    pub stake_vault: Pubkey,
    pub sequencer: Pubkey,
    pub oracle_program: Pubkey,
    pub guardian: Pubkey,
    pub team: Pubkey,
    pub admins: [Pubkey; 3],
    pub paused: bool,
    /// True only after loader-v3 authority has been irreversibly removed.
    pub immutable: bool,
    pub permissionless: bool,
    pub started_at: i64,
    pub last_sequenced_at: i64,
    pub max_leverage: u16,
    pub governance_delay: i64,
    pub next_ticket: u64,
    pub execute_ticket: u64,
    pub next_position: u64,
    pub next_claim: u64,
    pub next_proposal: u64,
    pub config_version: u64,
    pub house_cash: u64,
    pub fee_team: u64,
    pub fee_staker: u64,
    pub total_stake: u64,
    pub total_pending_stake: u64,
    pub stake_warmup: i64,
    pub reward_index: u128,
    pub reward_remainder: u64,
    pub tracked_lp: i128,
    pub tail_highwater: u64,
    pub total_token_credits: u64,
    pub claim_head: u64,
    pub cash_loss_credits: u64,
    pub debt_loss_credits: u64,
    /// Highest distributable equity after reserving open winning exposure.
    pub lp_highwater: u64,
    /// Unallocated net cash-funded profit. Destroyed IOUs cannot increase it.
    pub lp_pending_profit: u64,
    pub totals: Totals,
    #[borsh(skip)]
    pub loaded_totals: Totals,
    #[borsh(skip)]
    pub loaded_exposures: Vec<Exposure>,
    pub markets: Vec<Market>,
    #[borsh(skip)]
    pub users: Vec<User>,
    #[borsh(skip)]
    pub positions: Vec<Position>,
    #[borsh(skip)]
    pub claims: Vec<Claim>,
    #[borsh(skip)]
    pub intents: Vec<Intent>,
    pub outcomes: Vec<Outcome>,
    #[borsh(skip)]
    pub proposals: Vec<Proposal>,
}
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum Instruction {
    Initialize {
        local_mode: bool,
        devnet: bool,
        sequencer: Pubkey,
        oracle_program: Pubkey,
        guardian: Pubkey,
        team: Pubkey,
        admins: [Pubkey; 3],
        governance_delay: i64,
        markets: Vec<Market>,
    },
    Deposit {
        amount: u64,
    },
    Withdraw {
        amount: u64,
    },
    SetDelegate {
        delegate: Pubkey,
        expiry: i64,
        budget: u64,
        max_leverage: u16,
        market_mask: u64,
    },
    RevokeDelegate,
    Enqueue {
        /// Ignored: the program assigns the ticket and market sequence when the
        /// admission lands and returns the ticket as return data.
        ticket: u64,
        market_seq: u64,
        owner: Pubkey,
        nonce: u64,
        market: u16,
        kind: u8,
        side: i8,
        margin: u64,
        leverage: u16,
        claim_id: u64,
        position_id: u64,
        expiry: i64,
        limit_price: u64,
        try_execute: bool,
    },
    Drain {
        max: u16,
    },
    Liquidate {
        position_id: u64,
    },
    Harvest {
        max: u16,
    },
    MintRewards,
    Stake {
        amount: u64,
    },
    Unstake {
        amount: u64,
    },
    ClaimStaker,
    ClaimTeam {
        amount: u64,
    },
    Pause,
    Propose {
        market: u16,
        max_spread_bps: u16,
        max_deviation_bps: u16,
        max_age_slots: u64,
        max_leverage: u16,
        position_multiplier_milli: u64,
        clear_pause: bool,
        fill_delay_slots: u16,
        fill_window_slots: u16,
    },
    Approve {
        proposal_id: u64,
    },
    Apply {
        proposal_id: u64,
    },
    ProposeUpgrade {
        buffer: Pubkey,
    },
    ApplyUpgrade {
        proposal_id: u64,
    },
    ActivateStake,
    RequestExit {
        position_id: u64,
        limit_price: u64,
    },
    EnqueueExit {
        position_id: u64,
    },
    LiquidateBatch {
        market: u16,
        position_ids: Vec<u64>,
    },
    ReclaimRecord {
        record: Pubkey,
    },
    ProposeRotation {
        sequencer: Option<Pubkey>,
        guardian: Option<Pubkey>,
        team: Option<Pubkey>,
        admin: Option<(u8, Pubkey)>,
    },
    ApplyRotation {
        proposal_id: u64,
    },
    SetGovernanceDelay {
        seconds: i64,
    },
    WithdrawRent {
        lamports: u64,
    },
    AddMarket {
        market: Market,
    },
    /// One-way lock: revokes the loader's upgrade authority and arms the recovery
    /// timers. Until then the program is upgradeable and the upgrade authority may
    /// add and configure markets. Legacy governance tags remain disabled tombstones.
    Seal,
    RemoveRelayer,
    CheckpointOracle {
        market: u16,
    },
    WindDownMarket {
        market: u16,
    },
    SettleWindDown {
        position_id: u64,
    },
    /// Restore a stopped market's reservation without waiting for the FIFO head.
    CancelWindDownIntent {
        ticket: u64,
    },
    /// Keeps an idle, healthy sequencing service from timing out.
    SequencerHeartbeat,
    /// Before sealing only, signed by the loader's upgrade authority: replace an
    /// existing market's oracle accounts and risk settings, and optionally lift a pause.
    ConfigureMarket {
        market: Market,
        clear_pause: bool,
    },
}
