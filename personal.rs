// src/instructions/bonus/personal.rs — v7.0 (ФИНАЛЬНАЯ РАБОЧАЯ ВЕРСИЯ)

use borsh::{BorshDeserialize, BorshSerialize};
use crate::{
    constants::*,
    error::WaterError,
    instructions::utils::account_utils::get_total_token_balance,
    instructions::utils::get_associated_token_address,
    state::{
        referral_chunk::ReferralChunk,
        mem_rank_config::MemRankConfig,
        user::User,
        team::Team,
        team_chunk::TeamChunk,
        config_trait::ConfigAccount,
        PlayerStakeData,
    },
    utils::validation_utils::{self, validate_root_pda_v2},
    types::MemRankRequirements,
    instructions::utilits::pda::{
        derive_bonus_fund_pda,
        derive_team_pda,
    },
};

use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint::ProgramResult,
    msg,
    program::invoke_signed,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar::Sysvar,
};
use spl_token_2022_interface::instruction as token_instruction;
use spl_token_2022_interface::state::Mint;
use spl_token_2022_interface::extension::StateWithExtensions;

#[inline(never)]
pub fn update_mem_rank<'a>(
    program_id: &Pubkey,
    accounts: &'a [AccountInfo<'a>],
) -> ProgramResult {
    let mut account_iter = accounts.iter();

    // ==================== 1. РАЗБОР АККАУНТОВ (14 аккаунтов) ====================
    let claimant_info = next_account_info(&mut account_iter)?;                    // 0
    let claimant_token_account_info = next_account_info(&mut account_iter)?;      // 1
    let user_account = next_account_info(&mut account_iter)?;                     // 2
    let token_mint_info = next_account_info(&mut account_iter)?;                  // 3
    let token_program_info = next_account_info(&mut account_iter)?;               // 4
    let mem_rank_config_info = next_account_info(&mut account_iter)?;             // 5
    let stake_data_account = next_account_info(&mut account_iter)?;               // 6
    let bonus_fund_account = next_account_info(&mut account_iter)?;               // 7
    let immutable_seed_account = next_account_info(&mut account_iter)?;           // 8
    let bonus_fund_ata = next_account_info(&mut account_iter)?;                   // 9
    let user_token_ata = next_account_info(&mut account_iter)?;                   // 10
    let associated_token_program = next_account_info(&mut account_iter)?;         // 11
    let team_chunk_account = next_account_info(&mut account_iter)?;               // 12
    let team_account = next_account_info(&mut account_iter)?;                     // 13

    // ==================== 2. ПРОВЕРКИ ====================
    if !claimant_info.is_signer {
        msg!("♒~~~⚡~~~:FEE:user={}:amount=0:reason=MISSING_SIGNATURE", claimant_info.key);
        return Err(ProgramError::MissingRequiredSignature);
    }

    for acc in &[user_account, mem_rank_config_info, bonus_fund_account, stake_data_account] {
        if acc.owner != program_id {
            msg!("❌ Неверный owner: {}", acc.key);
            return Err(ProgramError::InvalidAccountOwner);
        }
    }
    
    for acc in &[claimant_token_account_info, bonus_fund_ata, user_token_ata] {
        if acc.owner != token_program_info.key {
            msg!("❌ Неверный owner ATA: {}", acc.key);
            return Err(ProgramError::InvalidAccountOwner);
        }
    }

    if *token_program_info.key != spl_token_2022_interface::ID {
        msg!("❌ Неверная токен программа!");
        return Err(ProgramError::IncorrectProgramId);
    }

    // Проверка mint
    {
        let mint_data = token_mint_info.data.borrow();
        let mint = StateWithExtensions::<Mint>::unpack(&mint_data)
            .map_err(|_| WaterError::InvalidAccountData)?;
        if mint.base.decimals != OUR_TOKEN_DECIMALS {
            msg!("❌ Неверный decimals!");
            return Err(WaterError::InvalidDecimals.into());
        }
    }

    let root_pda = validate_root_pda_v2(program_id, immutable_seed_account)?;
    let mem_rank_config = MemRankConfig::get_config(mem_rank_config_info, program_id, &root_pda)?;

    // Проверки PDA
    let (expected_user_pda, _) = User::find_user_address(program_id, claimant_info.key);
    if user_account.key != &expected_user_pda {
        msg!("❌ Неверный User PDA!");
        return Err(WaterError::InvalidPDA.into());
    }

    let (expected_stake_pda, _) = PlayerStakeData::find_stake_data_address(program_id, claimant_info.key);
    if stake_data_account.key != &expected_stake_pda {
        msg!("❌ Неверный Stake Data PDA!");
        return Err(WaterError::InvalidPDA.into());
    }

    let expected_claimant_ata = get_associated_token_address(claimant_info.key, token_mint_info.key);
    if claimant_token_account_info.key != &expected_claimant_ata {
        msg!("❌ Неверный claimant ATA!");
        return Err(WaterError::InvalidPDA.into());
    }

    let expected_user_ata = get_associated_token_address(claimant_info.key, token_mint_info.key);
    if user_token_ata.key != &expected_user_ata {
        msg!("❌ Неверный User ATA!");
        return Err(WaterError::InvalidPDA.into());
    }

    let (expected_bonus_fund, bonus_bump) = derive_bonus_fund_pda(program_id, &root_pda);
    if bonus_fund_account.key != &expected_bonus_fund {
        msg!("❌ Неверный Bonus Fund PDA!");
        return Err(WaterError::InvalidPDA.into());
    }

    let expected_bonus_ata = get_associated_token_address(&expected_bonus_fund, token_mint_info.key);
    if bonus_fund_ata.key != &expected_bonus_ata {
        msg!("❌ Неверный Bonus Fund ATA!");
        return Err(WaterError::InvalidPDA.into());
    }

    // ==================== 3. ЗАГРУЗКА ПОЛЬЗОВАТЕЛЯ ====================
    let mut user = User::get_user(user_account, program_id)?;
    if user.user_pubkey != *claimant_info.key {
        msg!("❌ User mismatch!");
        return Err(WaterError::InvalidAccountData.into());
    }

    let current_time = Clock::get()?.unix_timestamp;
    let old_rank = user.current_rank;

    // ==================== 4. СБОР ДАННЫХ ====================
    let total_tokens = get_total_token_balance(claimant_token_account_info, stake_data_account)?;
    user.referral_tree_size = ReferralChunk::get_referral_tree_size(program_id, &user.user_pubkey, accounts)?;

    // ==================== 5. ПРОВЕРКА ПОВЫШЕНИЯ (+1) ====================
    let mut new_rank = user.current_rank;
    let mut rank_updated = false;
    
    if new_rank < MAX_MEM_RANKS as u8 - 1 {
        let next_req = &mem_rank_config.mem_ranks[(new_rank + 1) as usize];
        if next_req.is_active
            && total_tokens >= next_req.min_tokens.into()
            && user.total_stake_hours >= next_req.min_stake_hours
            && user.referral_tree_size >= u32::from(next_req.min_referral_tree_size)
            && check_member_requirements(
                program_id,
                &user,
                next_req,
                team_chunk_account,
                team_account,
                accounts,
            )?
        {
            new_rank += 1;
            rank_updated = true;
        }
    }
    
    // ==================== 6. ВЫПЛАТА БОНУСА ИЗ BONUS_FUND ====================
    if rank_updated && new_rank > old_rank {
        user.current_rank = new_rank;
        user.last_rank_update = current_time;
        
        if !user.personal_rewards_claimed[new_rank as usize] {
            let reward_amount = mem_rank_config.mem_rank_rewards[new_rank as usize];
            if reward_amount > 0 {
                if get_token_balance(bonus_fund_ata)? < reward_amount {
                    msg!("❌ Недостаточно средств в Bonus Fund!");
                    return Err(WaterError::InsufficientFunds.into());
                }
                
                let transfer_ix = token_instruction::transfer_checked(
                    token_program_info.key,
                    bonus_fund_ata.key,
                    token_mint_info.key,
                    user_token_ata.key,
                    bonus_fund_account.key,
                    &[],
                    reward_amount,
                    OUR_TOKEN_DECIMALS,
                )?;
                
                let bump_arr = [bonus_bump];
                let seeds: &[&[&[u8]]] = &[&[BONUS_FUND_SEED, root_pda.as_ref(), &[0u8], &bump_arr]];
                
                invoke_signed(
                    &transfer_ix,
                    &[
                        bonus_fund_ata.clone(),
                        user_token_ata.clone(),
                        token_mint_info.clone(),
                        bonus_fund_account.clone(),
                        token_program_info.clone(),
                    ],
                    seeds,
                )?;
                
                user.personal_rewards_claimed[new_rank as usize] = true;
                
                msg!("♒~~~💧💧💧~~~:PAID:user={}:rank={}:amount={}",
                    user.user_pubkey, new_rank, reward_amount);
            }
        }
    }

    // ==================== 7. ПОНИЖЕНИЕ ====================
    if !rank_updated {
        let mut check_rank = user.current_rank;
        while check_rank > 0 {
            let req = &mem_rank_config.mem_ranks[check_rank as usize];
            if req.is_active
                && total_tokens >= u64::from(req.min_tokens)
                && user.total_stake_hours >= req.min_stake_hours
                && user.referral_tree_size >= u32::from(req.min_referral_tree_size)
                && check_member_requirements(
                    program_id,
                    &user,
                    req,
                    team_chunk_account,
                    team_account,
                    accounts,
                )?
            {
                break;
            }
            check_rank -= 1;
        }
        if check_rank < user.current_rank {
            user.current_rank = check_rank;
            user.last_rank_update = current_time;
            msg!("♒~~~💧💧💧~~~:RANK_DOWN:user={}:old_rank={}:new_rank={}",
                user.user_pubkey, old_rank, user.current_rank);
        }
    }

    // ==================== 8. СОХРАНЕНИЕ ====================
    user.serialize(&mut *user_account.data.borrow_mut())?;
    Ok(())
}

// ==================== ВСПОМОГАТЕЛЬНЫЕ ФУНКЦИИ ====================

fn get_token_balance(account: &AccountInfo) -> Result<u64, WaterError> {
    crate::instructions::utils::account_utils::get_token_balance(account)
        .map_err(|_| WaterError::InvalidAccountData)
}

fn check_member_requirements<'a>(
    program_id: &Pubkey,
    user: &User,
    req: &MemRankRequirements,
    team_chunk_account: &AccountInfo<'a>,
    team_account: &AccountInfo<'a>,
    referral_accounts: &'a [AccountInfo<'a>],
) -> Result<bool, WaterError> {
    let has_requirements = req.required_members.iter().any(|&r| r > 0);
    if !has_requirements {
        return Ok(true);
    }

    let mut rank_counts = [0u32; MAX_MEM_RANKS];
    for page in 0..MAX_REFERRAL_PAGES {
        let (chunk_pda, _) = ReferralChunk::find_chunk_pda(program_id, &user.user_pubkey, page as u8);
        for acc in referral_accounts {
            if acc.key != &chunk_pda || acc.data_is_empty() {
                continue;
            }
            if let Ok(chunk) = ReferralChunk::try_from_slice(&acc.data.borrow()) {
                for entry in chunk.entries.iter().flatten() {
                    let (child_pda, _) = User::find_user_address(program_id, &entry.child);
                    for child_acc in referral_accounts {
                        if child_acc.key != &child_pda { continue; }
                        if child_acc.owner != program_id { continue; }
                        if let Ok(child) = User::try_from_slice(&child_acc.data.borrow()) {
                            if (child.current_rank as usize) < MAX_MEM_RANKS {
                                rank_counts[child.current_rank as usize] += 1;
                            }
                        }
                    }
                }
            }
            break;
        }
    }

    for (rank_idx, &required) in req.required_members.iter().enumerate() {
        if required > 0 {
            if rank_counts[rank_idx] < required as u32 {
                return Ok(false);
            }
        }
    }

    Ok(true)
}