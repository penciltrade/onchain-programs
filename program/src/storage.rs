//! Version 3 record PDAs. Only records supplied to a transaction are decoded.
//! Global funded/debt totals are adjusted by the delta of that loaded subset.
use crate::{ledger, types::*};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program::invoke_signed,
    program_error::ProgramError, pubkey::Pubkey, rent::Rent, system_instruction, system_program,
    sysvar::Sysvar,
};

pub const USER: u8 = 1;
pub const POSITION: u8 = 2;
pub const CLAIM: u8 = 3;
pub const INTENT: u8 = 4;
pub const PROPOSAL: u8 = 5;
const HEADER: usize = 33; // kind and original rent payer
pub struct Snapshot(pub Vec<(Pubkey, Vec<u8>)>);

fn prefix(kind: u8) -> Result<&'static [u8], ProgramError> {
    match kind {
        USER => Ok(b"user"),
        POSITION => Ok(b"position"),
        CLAIM => Ok(b"claim"),
        INTENT => Ok(b"intent"),
        PROPOSAL => Ok(b"proposal"),
        _ => Err(ProgramError::InvalidAccountData),
    }
}
pub fn address(program: &Pubkey, kind: u8, key: &[u8]) -> Result<(Pubkey, u8), ProgramError> {
    Ok(Pubkey::find_program_address(&[prefix(kind)?, key], program))
}
/// An intent's record is keyed by its owner and nonce, both fixed when the
/// owner signs. Its ticket is assigned when the admission lands, so the record
/// address cannot depend on it.
pub fn intent_key(owner: &Pubkey, nonce: u64) -> Vec<u8> {
    solana_program::hash::hashv(&[owner.as_ref(), &nonce.to_le_bytes()])
        .to_bytes()
        .to_vec()
}
fn decode<T: BorshDeserialize>(data: &[u8]) -> Result<T, ProgramError> {
    T::try_from_slice(data).map_err(|_| ProgramError::InvalidAccountData)
}
pub fn load(
    program: &Pubkey,
    accounts: &[AccountInfo],
    state: &mut State,
) -> Result<Snapshot, ProgramError> {
    let mut snapshot = Snapshot(Vec::new());
    for a in accounts {
        if a.owner != program || a.key == accounts[0].key {
            continue;
        }
        if snapshot.0.iter().any(|(k, _)| k == a.key) {
            continue;
        }
        let d = a.try_borrow_data()?;
        if d.len() < HEADER {
            return Err(ProgramError::InvalidAccountData);
        }
        if d[0] & 128 != 0 {
            continue;
        } // closed record, reclaimable by its original payer
        let seed = match d[0] {
            USER => {
                let u: User = decode(&d[HEADER..])?;
                let seed = u.owner.to_bytes().to_vec();
                state.users.push(u);
                seed
            }
            POSITION => {
                let p: Position = decode(&d[HEADER..])?;
                let seed = p.id.to_le_bytes().to_vec();
                state.positions.push(p);
                seed
            }
            CLAIM => {
                let c: Claim = decode(&d[HEADER..])?;
                let seed = c.id.to_le_bytes().to_vec();
                state.claims.push(c);
                seed
            }
            INTENT => {
                let i: Intent = decode(&d[HEADER..])?;
                let seed = intent_key(&i.owner, i.nonce);
                state.intents.push(i);
                seed
            }
            PROPOSAL => {
                let p: Proposal = decode(&d[HEADER..])?;
                let seed = p.id.to_le_bytes().to_vec();
                state.proposals.push(p);
                seed
            }
            _ => return Err(ProgramError::InvalidAccountData),
        };
        if address(program, d[0], &seed)?.0 != *a.key {
            return Err(ProgramError::InvalidSeeds);
        }
        snapshot.0.push((*a.key, d.to_vec()));
    }
    state.intents.sort_by_key(|i| i.ticket);
    state.claims.sort_by_key(|c| c.id);
    state.loaded_totals = ledger::loaded_totals(state)?;
    state.loaded_exposures = ledger::loaded_exposures(state)?;
    // Check that the global totals cover all loaded records before any mutation.
    ledger::current_totals(state)?;
    ledger::current_exposures(state)?;
    Ok(snapshot)
}
struct Float {
    owner: Pubkey,
    key: Pubkey,
    free: u64,
}
fn move_lamports(
    accounts: &[AccountInfo],
    from: &Pubkey,
    to: &Pubkey,
    amount: u64,
) -> ProgramResult {
    let find = |k: &Pubkey| {
        accounts
            .iter()
            .find(|a| a.key == k)
            .ok_or(ProgramError::NotEnoughAccountKeys)
    };
    let (from, to) = (find(from)?, find(to)?);
    let left = ledger::sub(from.lamports(), amount)?;
    let total = ledger::add(to.lamports(), amount)?;
    **from.try_borrow_mut_lamports()? = left;
    **to.try_borrow_mut_lamports()? = total;
    Ok(())
}
/// Pay out part of the signer's rent float. Call after the last CPI.
pub fn withdraw_float(program: &Pubkey, accounts: &[AccountInfo], lamports: u64) -> ProgramResult {
    let actor = &accounts[1];
    let key = address(program, USER, actor.key.as_ref())?.0;
    let user = accounts
        .iter()
        .find(|a| a.key == &key)
        .ok_or(ProgramError::NotEnoughAccountKeys)?;
    if user.owner != program || !user.is_writable || !actor.is_writable || !actor.is_signer {
        return Err(ProgramError::InvalidAccountData);
    }
    let free = user
        .lamports()
        .saturating_sub(Rent::get()?.minimum_balance(user.data_len()));
    if lamports == 0 || lamports > free {
        return Err(ProgramError::InsufficientFunds);
    }
    move_lamports(accounts, &key, actor.key, lamports)
}
struct Donor {
    key: Pubkey,
    kind: u8,
    owner: Pubkey,
    payer: Pubkey,
    used: bool,
    target: Pubkey,
}
/// The trader a record economically belongs to, for rent chaining.
pub fn record_owner_of(kind: u8, body: &[u8]) -> Option<Pubkey> {
    match kind {
        POSITION => Position::try_from_slice(body).ok().map(|p| p.owner),
        CLAIM => Claim::try_from_slice(body).ok().map(|c| c.owner),
        INTENT => Intent::try_from_slice(body).ok().map(|i| i.owner),
        _ => None,
    }
}
fn record_owner(data: &[u8]) -> Option<Pubkey> {
    record_owner_of(data[0] & 127, data.get(HEADER..)?)
}
/// Which closing records may fund a new record, most direct predecessor first.
pub fn donor_preference(kind: u8) -> Option<&'static [u8]> {
    match kind {
        POSITION => Some(&[INTENT]),
        CLAIM => Some(&[POSITION, INTENT]),
        _ => None,
    }
}
fn encode<T: BorshSerialize>(
    kind: u8,
    seed: Vec<u8>,
    record: &T,
) -> Result<(u8, Vec<u8>, Vec<u8>), ProgramError> {
    Ok((
        kind,
        seed,
        borsh::to_vec(record).map_err(|_| ProgramError::InvalidAccountData)?,
    ))
}
pub fn save(
    program: &Pubkey,
    accounts: &[AccountInfo],
    s: &mut State,
    before: Snapshot,
) -> ProgramResult {
    let mut records = Vec::new();
    for u in &s.users {
        records.push(encode(USER, u.owner.to_bytes().to_vec(), u)?);
    }
    for p in &s.positions {
        records.push(encode(POSITION, p.id.to_le_bytes().to_vec(), p)?);
    }
    for c in &s.claims {
        records.push(encode(CLAIM, c.id.to_le_bytes().to_vec(), c)?);
    }
    for i in &s.intents {
        records.push(encode(INTENT, intent_key(&i.owner, i.nonce), i)?);
    }
    for p in &s.proposals {
        records.push(encode(PROPOSAL, p.id.to_le_bytes().to_vec(), p)?);
    }
    let system = accounts.get(10).ok_or(ProgramError::NotEnoughAccountKeys)?;
    let payer = accounts.get(11).ok_or(ProgramError::NotEnoughAccountKeys)?;
    if system.key != &system_program::id() || !system.executable || !payer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    let mut addressed = Vec::with_capacity(records.len());
    for (kind, seed, data) in records {
        let (key, bump) = address(program, kind, &seed)?;
        addressed.push((kind, seed, data, key, bump));
    }
    let kept: Vec<Pubkey> = addressed.iter().map(|r| r.3).collect();
    // Records closing in this instruction can fund the same owner's successor
    // record (intent -> position -> claim), so a third-party drainer or keeper
    // never carries persistent rent for another trader's records.
    let mut donors: Vec<Donor> = before
        .0
        .iter()
        .filter(|(k, _)| !kept.contains(k))
        .filter_map(|(k, d)| {
            Some(Donor {
                key: *k,
                kind: d[0] & 127,
                owner: record_owner(d)?,
                payer: Pubkey::new_from_array(d[1..HEADER].try_into().ok()?),
                used: false,
                target: Pubkey::default(),
            })
        })
        .collect();
    // Rent float: whatever a user's record holds above its own rent-exempt
    // minimum pays for that user's order records and gets it back when they
    // close, so one small SOL deposit keeps circulating and neither the wallet
    // nor a session key is drained by rent.
    let rent = Rent::get()?;
    let mut floats: Vec<Float> = Vec::new();
    for (kind, seed, _, key, _) in &addressed {
        if *kind != USER || !before.0.iter().any(|(k, _)| k == key) {
            continue;
        }
        if let Some(a) = accounts.iter().find(|a| a.key == key && a.is_writable) {
            floats.push(Float {
                owner: Pubkey::new_from_array(
                    seed.as_slice()
                        .try_into()
                        .map_err(|_| ProgramError::InvalidSeeds)?,
                ),
                key: *key,
                free: a
                    .lamports()
                    .saturating_sub(rent.minimum_balance(a.data_len())),
            });
        }
    }
    let mut draws: Vec<(Pubkey, Pubkey, u64)> = Vec::new();
    for (kind, seed, data, key, bump) in addressed {
        let account = accounts
            .iter()
            .find(|a| a.key == &key)
            .ok_or(ProgramError::NotEnoughAccountKeys)?;
        let old = before.0.iter().find(|(k, _)| k == &key).map(|(_, v)| v);
        if let Some(old) = old {
            if old[HEADER..] == data {
                continue;
            }
        }
        if !account.is_writable {
            return Err(ProgramError::InvalidAccountData);
        }
        let mut donor_funding = 0u64;
        let mut rent_payer = old
            .map(|v| Pubkey::new_from_array(v[1..HEADER].try_into().unwrap()))
            .unwrap_or(*payer.key);
        if old.is_none() {
            if account.owner != &system_program::id()
                || !account.data_is_empty()
                || account.executable
            {
                return Err(ProgramError::AccountAlreadyInitialized);
            }
            let owner = record_owner_of(kind, &data);
            if let (Some(owner), Some(order)) = (owner, donor_preference(kind)) {
                let pick = order.iter().find_map(|k| {
                    donors
                        .iter()
                        .position(|d| !d.used && d.kind == *k && d.owner == owner)
                });
                if let Some(i) = pick {
                    let donor = accounts
                        .iter()
                        .find(|a| a.key == &donors[i].key)
                        .ok_or(ProgramError::NotEnoughAccountKeys)?;
                    if !donor.is_writable {
                        return Err(ProgramError::InvalidAccountData);
                    }
                    // The balance itself moves after the last CPI below: Agave
                    // rejects a CPI while the caller's own lamport edits are
                    // unreconciled in accounts the callee cannot see.
                    donor_funding = donor.lamports();
                    rent_payer = donors[i].payer;
                    donors[i].used = true;
                    donors[i].target = key;
                }
            }
        }
        let needed = if old.is_none() {
            rent.minimum_balance(HEADER + data.len())
                .saturating_sub(ledger::add(account.lamports(), donor_funding)?)
        } else {
            0
        };
        let mut from_payer = needed;
        if needed > 0 {
            let float = record_owner_of(kind, &data)
                .and_then(|owner| floats.iter_mut().find(|f| f.owner == owner));
            if let Some(f) = float.filter(|f| f.free >= needed) {
                // Moved after the last CPI, like a donor's balance.
                f.free -= needed;
                draws.push((f.key, key, needed));
                if donor_funding == 0 {
                    rent_payer = f.key;
                }
                from_payer = 0;
            }
        }
        let mut bytes = vec![kind];
        bytes.extend_from_slice(rent_payer.as_ref());
        bytes.extend_from_slice(&data);
        if old.is_none() {
            if from_payer > 0 {
                solana_program::program::invoke(
                    &system_instruction::transfer(payer.key, &key, from_payer),
                    &[payer.clone(), account.clone(), system.clone()],
                )?;
            }
            let seeds = &[prefix(kind)?, seed.as_slice(), &[bump]];
            invoke_signed(
                &system_instruction::allocate(&key, bytes.len() as u64),
                &[account.clone(), system.clone()],
                &[seeds],
            )?;
            invoke_signed(
                &system_instruction::assign(&key, program),
                &[account.clone(), system.clone()],
                &[seeds],
            )?;
        }
        if account.data_len() != bytes.len() {
            return Err(ProgramError::InvalidAccountData);
        }
        account.try_borrow_mut_data()?.copy_from_slice(&bytes);
    }
    // No CPI follows this point, so balances may now move directly.
    for (from, to, amount) in draws {
        move_lamports(accounts, &from, &to, amount)?;
    }
    for (key, data) in before.0 {
        if kept.contains(&key) {
            continue;
        }
        let a = accounts
            .iter()
            .find(|a| a.key == &key)
            .ok_or(ProgramError::NotEnoughAccountKeys)?;
        if !a.is_writable {
            return Err(ProgramError::InvalidAccountData);
        }
        // A record funded from its owner's float returns the rent there and is
        // deleted outright; nothing is left behind to reclaim.
        let funder = Pubkey::new_from_array(data[1..HEADER].try_into().unwrap());
        let home = record_owner(&data)
            .and_then(|owner| floats.iter().find(|f| f.owner == owner && f.key == funder));
        if let (Some(f), false) = (home, donors.iter().any(|d| d.used && d.key == key)) {
            move_lamports(accounts, &key, &f.key, a.lamports())?;
            a.realloc(0, false)?;
            a.assign(&system_program::id());
            continue;
        }
        if let Some(d) = donors.iter().find(|d| d.used && d.key == key) {
            // No CPI follows this point. Move the donor's whole balance (a
            // partial drain would leave it below rent exemption) into its
            // successor record, then delete the donor outright.
            let target = accounts
                .iter()
                .find(|x| x.key == &d.target)
                .ok_or(ProgramError::NotEnoughAccountKeys)?;
            let moved = a.lamports();
            **a.try_borrow_mut_lamports()? = 0;
            **target.try_borrow_mut_lamports()? = ledger::add(target.lamports(), moved)?;
            a.realloc(0, false)?;
            a.assign(&system_program::id());
            continue;
        }
        let mut d = a.try_borrow_mut_data()?;
        d.fill(0);
        d[..HEADER].copy_from_slice(&data[..HEADER]);
        d[0] |= 128;
    }
    s.totals = ledger::current_totals(s)?;
    s.loaded_totals = ledger::loaded_totals(s)?;
    let exposures = ledger::current_exposures(s)?;
    for (m, e) in s.markets.iter_mut().zip(&exposures) {
        m.long_units = e.long_units;
        m.short_notional = e.short_notional;
    }
    s.loaded_exposures = ledger::loaded_exposures(s)?;
    Ok(())
}
pub fn reclaim(
    program: &Pubkey,
    accounts: &[AccountInfo],
    key: &Pubkey,
    claim_head: u64,
) -> ProgramResult {
    let a = accounts
        .iter()
        .find(|a| a.key == key)
        .ok_or(ProgramError::NotEnoughAccountKeys)?;
    let actor = &accounts[1];
    if a.owner != program || !a.is_writable || !actor.is_writable || !actor.is_signer {
        return Err(ProgramError::InvalidAccountData);
    }
    let d = a.try_borrow_data()?;
    // The original payer reclaims; for a record funded from a rent float that
    // is the user whose float it was.
    if d.len() < HEADER
        || (&d[1..HEADER] != actor.key.as_ref()
            && d[1..HEADER] != address(program, USER, actor.key.as_ref())?.0.to_bytes())
    {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if d[0] & 128 == 0 {
        if d[0] != CLAIM {
            return Err(ProgramError::InvalidArgument);
        }
        let c: Claim = decode(&d[HEADER..])?;
        if c.id >= claim_head || c.amount != 0 || c.reserved != 0 {
            return Err(ProgramError::InvalidArgument);
        }
    }
    drop(d);
    **actor.try_borrow_mut_lamports()? = ledger::add(actor.lamports(), a.lamports())?;
    **a.try_borrow_mut_lamports()? = 0;
    a.realloc(0, false)?;
    a.assign(&system_program::id());
    Ok(())
}
