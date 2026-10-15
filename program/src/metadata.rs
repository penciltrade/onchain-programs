//! One-time creation of PENCIL display metadata. No custody or state mutation.
use super::*;

pub const PROGRAM: Pubkey = solana_program::pubkey!("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");
pub const NAME: &str = "PENCIL";
pub const SYMBOL: &str = "PENCIL";
pub const URI: &str = "https://penciltrade.fun/pencil-token.json";

fn data() -> Vec<u8> {
    // Metaplex CreateMetadataAccountV3: DataV2, mutable, no collection details.
    let mut bytes = vec![33];
    for value in [NAME, SYMBOL, URI] {
        bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
        bytes.extend_from_slice(value.as_bytes());
    }
    // Zero royalties; no creators, collection, or uses. The payer can freeze
    // metadata after wallet display checks, without another program upgrade.
    bytes.extend_from_slice(&[0, 0, 0, 0, 0, 1, 0]);
    bytes
}

pub fn create(id: &Pubkey, a: &[AccountInfo]) -> ProgramResult {
    let root = account(a, 0)?;
    let payer = account(a, 1)?;
    let mint = account(a, 2)?;
    let metadata = account(a, 3)?;
    let program = account(a, 4)?;
    let system = account(a, 5)?;
    let (expected, bump) = Pubkey::find_program_address(&[b"state"], id);
    if root.key != &expected || root.owner != id {
        return Err(ProgramError::InvalidAccountData);
    }
    let state = read_state(root)?;
    if state.bump != bump {
        return Err(ProgramError::InvalidSeeds);
    }
    if state.immutable {
        return Err(ProgramError::Custom(IMMUTABLE));
    }
    signer(payer)?;
    if !payer.is_writable
        || loader_authority(id, account(a, 6)?, account(a, 7)?)? != Some(*payer.key)
    {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if mint.key != &state.pencil_mint
        || mint.owner != &spl_token::id()
        || program.key != &PROGRAM
        || !program.executable
        || system.key != &solana_program::system_program::id()
        || !system.executable
    {
        return Err(ProgramError::IncorrectProgramId);
    }
    let token = Mint::unpack(&mint.try_borrow_data()?)?;
    if token.decimals != 6
        || token.mint_authority != solana_program::program_option::COption::Some(expected)
        || token.freeze_authority.is_some()
    {
        return Err(ProgramError::InvalidAccountData);
    }
    let expected_metadata = Pubkey::find_program_address(
        &[b"metadata", PROGRAM.as_ref(), mint.key.as_ref()],
        &PROGRAM,
    )
    .0;
    if metadata.key != &expected_metadata || !metadata.is_writable {
        return Err(ProgramError::InvalidSeeds);
    }
    if metadata.owner != &solana_program::system_program::id() || !metadata.data_is_empty() {
        return Err(ProgramError::AccountAlreadyInitialized);
    }
    let instruction = SolIx {
        program_id: PROGRAM,
        accounts: vec![
            AccountMeta::new(*metadata.key, false),
            AccountMeta::new_readonly(*mint.key, false),
            AccountMeta::new_readonly(expected, true),
            AccountMeta::new(*payer.key, true),
            AccountMeta::new_readonly(*payer.key, true),
            AccountMeta::new_readonly(*system.key, false),
        ],
        data: data(),
    };
    invoke_signed(
        &instruction,
        &[
            metadata.clone(),
            mint.clone(),
            root.clone(),
            payer.clone(),
            system.clone(),
            program.clone(),
        ],
        &[&[b"state", &[bump]]],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_program::{
        program_option::COption,
        program_stubs::{set_syscall_stubs, SyscallStubs},
    };
    use std::sync::{Arc, Mutex};

    struct Capture(Arc<Mutex<Vec<SolIx>>>);
    impl SyscallStubs for Capture {
        fn sol_invoke_signed(
            &self,
            ix: &SolIx,
            infos: &[AccountInfo],
            seeds: &[&[&[u8]]],
        ) -> ProgramResult {
            assert_eq!(seeds.len(), 1);
            assert_eq!(
                Pubkey::create_program_address(seeds[0], infos[2].owner).unwrap(),
                *infos[2].key
            );
            self.0.lock().unwrap().push(ix.clone());
            Ok(())
        }
    }
    struct Restore(Option<Box<dyn SyscallStubs>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            set_syscall_stubs(self.0.take().unwrap());
        }
    }

    fn attempt(change: &str) -> (ProgramResult, Vec<SolIx>) {
        let _lock = crate::recovery_tests::SYSCALLS
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let captured = Arc::new(Mutex::new(Vec::new()));
        let _restore = Restore(Some(set_syscall_stubs(Box::new(Capture(captured.clone())))));
        let id = Pubkey::new_unique();
        let (root, bump) = Pubkey::find_program_address(&[b"state"], &id);
        let payer = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let pd = Pubkey::find_program_address(&[id.as_ref()], &bpf_loader_upgradeable::id()).0;
        let metadata =
            Pubkey::find_program_address(&[b"metadata", PROGRAM.as_ref(), mint.as_ref()], &PROGRAM)
                .0;
        let fixture = include_str!("../../tests/fixtures/v4-state.hex").trim();
        let bytes: Vec<u8> = (0..fixture.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&fixture[i..i + 2], 16).unwrap())
            .collect();
        let n = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        let mut state = State::try_from_slice(&bytes[4..4 + n]).unwrap();
        state.bump = bump;
        state.pencil_mint = mint;
        state.immutable = change == "sealed";
        let encoded = borsh::to_vec(&state).unwrap();
        let mut data = vec![Vec::new(); 8];
        data[0] = [
            (encoded.len() as u32).to_le_bytes().as_slice(),
            encoded.as_slice(),
        ]
        .concat();
        data[2] = vec![0; Mint::LEN];
        Mint::pack(
            Mint {
                mint_authority: COption::Some(root),
                supply: 123456789,
                decimals: 6,
                is_initialized: true,
                freeze_authority: COption::None,
            },
            &mut data[2],
        )
        .unwrap();
        data[6] = [2u32.to_le_bytes().as_slice(), pd.as_ref()].concat();
        data[7] = [
            3u32.to_le_bytes().as_slice(),
            1u64.to_le_bytes().as_slice(),
            &[1],
            payer.as_ref(),
        ]
        .concat();
        let system = solana_program::system_program::id();
        let loader = bpf_loader_upgradeable::id();
        let mut keys = vec![root, payer, mint, metadata, PROGRAM, system, id, pd];
        let mut owners = vec![
            id,
            system,
            spl_token::id(),
            system,
            loader,
            system,
            loader,
            loader,
        ];
        if change == "payer" {
            keys[1] = Pubkey::new_unique();
        }
        if change == "metadata" {
            keys[3] = Pubkey::new_unique();
        }
        if change == "program" {
            keys[4] = Pubkey::new_unique();
        }
        if change == "mint" {
            keys[2] = Pubkey::new_unique();
        }
        if change == "root" {
            keys[0] = Pubkey::new_unique();
        }
        if change == "loader" {
            owners[7] = system;
        }
        if change == "existing" {
            owners[3] = PROGRAM;
            data[3] = vec![4; 679];
        }
        let mut lamports = vec![1_000_000_000; 8];
        let before = data.clone();
        let infos: Vec<_> = keys
            .iter()
            .zip(owners.iter())
            .zip(lamports.iter_mut())
            .zip(data.iter_mut())
            .enumerate()
            .map(|(i, (((key, owner), balance), bytes))| {
                AccountInfo::new(
                    key,
                    i == 1 && change != "unsigned",
                    i == 1 || i == 3,
                    balance,
                    bytes,
                    owner,
                    [4, 5, 6].contains(&i),
                    0,
                )
            })
            .collect();
        let result = process_instruction(
            &id,
            &infos,
            &borsh::to_vec(&Instruction::CreatePencilMetadata).unwrap(),
        );
        drop(infos);
        assert_eq!(
            data, before,
            "Metadata creation never writes protocol or mint data"
        );
        let calls = captured.lock().unwrap().clone();
        (result, calls)
    }

    #[test]
    fn metadata_creation_uses_only_fixed_fields_and_the_state_signer() {
        let (result, calls) = attempt("");
        assert_eq!(result, Ok(()));
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].program_id, PROGRAM);
        assert_eq!(calls[0].data, data());
        assert_eq!(calls[0].accounts.len(), 6);
        assert!(!calls[0].accounts[1].is_writable);
        assert!(calls[0].accounts[2].is_signer);
        assert_eq!(calls[0].accounts[3].pubkey, calls[0].accounts[4].pubkey);
        assert!(calls[0].accounts[3].is_writable);
        assert!(!calls[0].accounts[4].is_writable);
    }

    #[test]
    fn metadata_creation_rejects_forged_accounts_and_post_seal_calls_before_cpi() {
        for change in [
            "payer", "metadata", "program", "mint", "root", "loader", "existing", "unsigned",
            "sealed",
        ] {
            let (result, calls) = attempt(change);
            assert!(result.is_err(), "{}", change);
            assert!(calls.is_empty(), "{}", change);
        }
    }
}
