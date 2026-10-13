//! Explicit test oracle for localnet and devnet. This program is NOT Phoenix and
//! never runs on mainnet: PENCILTRADE refuses it unless initialized in local or
//! devnet mode. It answers Hawkeye's `view_bbo` call with the same 64-byte
//! return layout, reading one authority-updated quote account per market.
#![allow(unexpected_cfgs)]
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint,
    entrypoint::ProgramResult,
    hash::hash,
    program::{invoke_signed, set_return_data},
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    system_instruction,
    sysvar::Sysvar,
};
entrypoint!(process_instruction);
#[derive(BorshSerialize, BorshDeserialize)]
struct Quote {
    authority: Pubkey,
    market: u16,
    bid: u64,
    ask: u64,
    mark: u64,
    index: u64,
    slot: u64,
}
fn process_instruction(id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    if data.len() == 8 && data == &hash(b"global:view_bbo").to_bytes()[..8] {
        // Hawkeye-compatible view: the market is identified by the account set.
        let q = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
        if q.owner != id {
            return Err(ProgramError::InvalidAccountData);
        }
        let value = Quote::try_from_slice(&q.try_borrow_data()?)
            .map_err(|_| ProgramError::InvalidAccountData)?;
        let (expected, _) =
            Pubkey::find_program_address(&[b"quote", &value.market.to_le_bytes()], id);
        if q.key != &expected {
            return Err(ProgramError::InvalidAccountData);
        }
        let mut ret = [0u8; 64];
        ret[..8].copy_from_slice(&[0x71, 0x41, 0xa7, 0x1f, 0xa3, 0x1f, 0xca, 0xef]);
        ret[8..10].copy_from_slice(&1u16.to_le_bytes());
        ret[10] = (if value.bid > 0 { 1 } else { 0 }) | (if value.ask > 0 { 2 } else { 0 });
        for (off, x) in [
            (16, value.bid),
            (24, value.ask),
            (32, value.mark),
            (40, value.index),
            (48, value.slot),
            (56, value.slot),
        ] {
            ret[off..off + 8].copy_from_slice(&x.to_le_bytes());
        }
        set_return_data(&ret);
        return Ok(());
    }
    if data.len() != 35 {
        return Err(ProgramError::InvalidInstructionData);
    }
    let op = data[0];
    let market = u16::from_le_bytes([data[1], data[2]]);
    let seed = market.to_le_bytes();
    let (expected, bump) = Pubkey::find_program_address(&[b"quote", &seed], id);
    let mut it = accounts.iter();
    let actor = next_account_info(&mut it)?;
    let q = next_account_info(&mut it)?;
    let system = next_account_info(&mut it)?;
    if !actor.is_signer || q.key != &expected {
        return Err(ProgramError::MissingRequiredSignature);
    }
    let read = |off: usize| u64::from_le_bytes(data[off..off + 8].try_into().unwrap());
    if op == 0 {
        if q.owner != &solana_program::system_program::id() || q.data_len() != 0 {
            return Err(ProgramError::AccountAlreadyInitialized);
        }
        let minimum = Rent::get()?.minimum_balance(74);
        if q.lamports() < minimum {
            solana_program::program::invoke(
                &system_instruction::transfer(actor.key, q.key, minimum - q.lamports()),
                &[actor.clone(), q.clone(), system.clone()],
            )?;
        }
        invoke_signed(
            &system_instruction::allocate(q.key, 74),
            &[q.clone(), system.clone()],
            &[&[b"quote", &seed, &[bump]]],
        )?;
        invoke_signed(
            &system_instruction::assign(q.key, id),
            &[q.clone(), system.clone()],
            &[&[b"quote", &seed, &[bump]]],
        )?;
    } else if op == 1 {
        if q.owner != id {
            return Err(ProgramError::IncorrectProgramId);
        }
        let prev = Quote::try_from_slice(&q.try_borrow_data()?)
            .map_err(|_| ProgramError::InvalidAccountData)?;
        if prev.authority != *actor.key {
            return Err(ProgramError::MissingRequiredSignature);
        }
    } else {
        return Err(ProgramError::InvalidInstructionData);
    }
    let value = Quote {
        authority: *actor.key,
        market,
        bid: read(3),
        ask: read(11),
        mark: read(19),
        index: read(27),
        slot: Clock::get()?.slot,
    };
    value
        .serialize(&mut &mut q.try_borrow_mut_data()?[..])
        .map_err(|_| ProgramError::InvalidAccountData)
}
