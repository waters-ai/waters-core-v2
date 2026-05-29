// src/instructions/bonus/distribute_page.rs — v3.0 FINAL
// 🔥 Дистрибуция командного бонуса по страницам TeamChunk
// sub_tag 253: комиссия (mode=3) для (chunk_page=0, sub_page=0)
// Читает доли из TeamTmpSnapshot (leader_tokens, team_tokens)
// Реальные трансферы: team_token_ata → user_token_ata
//
// Аккаунты:
// 0. [signer] payer — любой член команды
// 1. [writable] team_account — Team PDA
// 2. [] snapshot_account — TeamTmpSnapshot PDA (доли)
// 3. [] team_chunk_account — TeamChunk (страница N)
// 4. [writable] team_token_ata — ATA команды (откуда)
// 5. [] token_mint
// 6. [] token_program
// ... [writable] user_token_ata × N — ATA участников
//
// Для (chunk_page=0, sub_page=0) — комиссия:
// + [writable] rent_pda
// + [] system_program
// + [writable] sol_fund
// + [] root_pda_account

use crate::{
    constants::*,
    error::WaterError,
    state::{
        team::Team,
        team_chunk::TeamChunk,
        team_tmp_snapshot::TeamTmpSnapshot,
        user::User,
    },
    instructions::utilits::pda::{
        derive_team_pda,
        derive_sol_fund_pda,
        get_root_pda_key,
    },
};
use crate::instructions::utils::get_associated_token_address;

use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar::{rent::Rent, Sysvar},
    msg,
};
use spl_token_2022_interface::instruction as token_instruction;
use solana_system_interface::instruction as system_instruction;

pub fn process_distribute_page(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    chunk_page: u8,
    sub_page: u8,
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    
    let payer = next_account_info(accounts_iter)?;               // 0
    let team_account = next_account_info(accounts_iter)?;        // 1
    let snapshot_account = next_account_info(accounts_iter)?;    // 2
    let team_chunk_account = next_account_info(accounts_iter)?;  // 3
    let team_token_ata = next_account_info(accounts_iter)?;      // 4
    let token_mint = next_account_info(accounts_iter)?;          // 5
    let token_program = next_account_info(accounts_iter)?;       // 6
    
    // Комиссия только для самого первого батча
    let (rent_pda, system_program, sol_fund, root_pda_account) = 
        if chunk_page == 0 && sub_page == 0 {
            let rp = next_account_info(accounts_iter)?;
            let sp = next_account_info(accounts_iter)?;
            let sf = next_account_info(accounts_iter)?;
            let rpa = next_account_info(accounts_iter)?;
            (Some(rp), Some(sp), Some(sf), Some(rpa))
        } else {
            (None, None, None, None)
        };
    
    let user_atas = accounts_iter.as_slice();
    
    // ==================== ПРОВЕРКИ ====================
    if !payer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    
    if team_account.owner != program_id || snapshot_account.owner != program_id {
        return Err(ProgramError::InvalidAccountOwner);
    }
    
    let team = Team::load(team_account)?;
    let (expected_team_pda, _) = derive_team_pda(program_id, &team.immutable.leader);
    if team_account.key != &expected_team_pda {
        return Err(WaterError::InvalidPDA.into());
    }
    
    // Проверка членства
    let (user_pda, _) = User::find_user_address(program_id, payer.key);
    let user_account = accounts.iter().find(|acc| acc.key == &user_pda)
        .ok_or(WaterError::InvalidAccountData)?;
    let user = User::get_user(user_account, program_id)?;
    
    if user.team_id.map_or(true, |tid| tid != expected_team_pda) {
        return Err(WaterError::Unauthorized.into());
    }
    
    // Проверка статуса бонуса
    let current_rank = team.mutable.current_rank;
    if !team.is_bonus_claimed(current_rank) {
        return Err(WaterError::MustCompleteDistribution.into());
    }
    if team.is_bonus_distributed(current_rank) {
        return Err(WaterError::MustCompleteDistribution.into());
    }
    
    // Загрузка снапшота для долей
    let snapshot = TeamTmpSnapshot::load(snapshot_account)?;
    
    if !snapshot.is_ready_for_distribution() {
        return Err(WaterError::SnapshotNotFinalized.into());
    }
    
    // Проверка PDA TeamChunk
    let (expected_chunk_pda, _) = TeamChunk::find_chunk_pda(
        program_id, &team.immutable.leader, chunk_page
    );
    if team_chunk_account.key != &expected_chunk_pda {
        return Err(WaterError::InvalidPDA.into());
    }
    
    // ==================== КОМИССИЯ (первый батч) ====================
    if chunk_page == 0 && sub_page == 0 {
        let rent = Rent::get()?;
        let team_rent = rent.minimum_balance(Team::LEN);
        let ata_rent = rent.minimum_balance(165);
        
        let chunk = TeamChunk::load(team_chunk_account)?;
        let chunk_size = TeamChunk::size_for_capacity(chunk.capacity);
        let chunk_rent = rent.minimum_balance(chunk_size);
        
        let commission = team_rent
            .checked_add(ata_rent)
            .and_then(|v| v.checked_add(chunk_rent))
            .ok_or(WaterError::InvalidTransfer)?;
        
        msg!("💰 Commission: {} lamports (Team={} + ATA={} + Chunk(cap={})={})",
            commission, team_rent, ata_rent, chunk.capacity, chunk_rent);
        
        if **payer.try_borrow_lamports()? < commission {
            return Err(WaterError::InsufficientBalance.into());
        }
        
        let rent_pda = rent_pda.unwrap();
        let system_program = system_program.unwrap();
        let sol_fund = sol_fund.unwrap();
        let root_pda_account = root_pda_account.unwrap();
        
        // Проверка SOL Fund
        let root_pda = get_root_pda_key(program_id, root_pda_account)?;
        let (expected_sol_fund, _) = derive_sol_fund_pda(program_id, &root_pda);
        if sol_fund.key != &expected_sol_fund || sol_fund.owner != program_id {
            return Err(WaterError::InvalidPDA.into());
        }
        
        invoke(
            &system_instruction::transfer(payer.key, rent_pda.key, commission),
            &[payer.clone(), rent_pda.clone(), system_program.clone()],
        )?;
        msg!("💰 PAID: {} lamports to shard", commission);
        
        // Возврат излишков
        let shard_balance = **rent_pda.try_borrow_lamports()?;
        let min_shard = rent.minimum_balance(rent_pda.data_len());
        let target = min_shard.checked_mul(5).ok_or(WaterError::InvalidTransfer)?;
        
        if shard_balance > target {
            let surplus = shard_balance - target;
            **rent_pda.try_borrow_mut_lamports()? = target;
            **sol_fund.try_borrow_mut_lamports()? = sol_fund.lamports()
                .checked_add(surplus)
                .ok_or(WaterError::InvalidTransfer)?;
            msg!("💰 Surplus {} lamports to SOL Fund", surplus);
        }
    }
    
    // ==================== ЗАГРУЗКА УЧАСТНИКОВ ====================
    let team_chunk = TeamChunk::load(team_chunk_account)?;
    let members = team_chunk.get_all_members(team_chunk_account)?;
    
    // Дробление на подстраницы
    let start_idx = (sub_page as usize) * 53;
    let end_idx = std::cmp::min(start_idx + 53, members.len());
    
    if start_idx >= members.len() {
        msg!("⚠️ Empty sub-page: chunk={}, sub={}", chunk_page, sub_page);
        return Ok(());
    }
    
    let batch_members = &members[start_idx..end_idx];
    
    // Данные для расчета долей
    let acc = snapshot.get_accumulated_data();
    let team_pool = acc.total_contribution.saturating_sub(acc.leader_contribution);
    
    let mut processed_count = 0u32;
    let mut page_distributed = 0u64;
    
    // ==================== ДИСТРИБУЦИЯ ====================
    for (i, member_pubkey) in batch_members.iter().enumerate() {
        if i >= user_atas.len() {
            break;
        }
        
        let user_ata = &user_atas[i];
        
        // Проверка ATA
        let expected_ata = get_associated_token_address(member_pubkey, token_mint.key);
        if user_ata.key != &expected_ata {
            continue;
        }
        
        // Contribution участника
        let (user_pda, _) = User::find_user_address(program_id, member_pubkey);
        let user_contrib = accounts.iter()
            .find(|acc| acc.key == &user_pda)
            .and_then(|ua| User::get_user(ua, program_id).ok())
            .map(|u| u.team_contribution)
            .unwrap_or(0);
        
        if user_contrib == 0 {
            continue;
        }
        
        // Расчет доли
        let share = if *member_pubkey == team.immutable.leader {
            snapshot.leader_tokens
        } else if team_pool > 0 {
            (user_contrib as u128 * snapshot.team_tokens as u128 / team_pool as u128) as u64
        } else {
            0
        };
        
        if share == 0 {
            continue;
        }
        
        // 🔥 ТРАНСФЕР ТОКЕНОВ
        let (team_pda, team_bump) = derive_team_pda(program_id, &team.immutable.leader);
        let seeds = &[b"team", team.immutable.leader.as_ref(), &[team_bump]];
        
        let transfer_ix = token_instruction::transfer_checked(
            token_program.key,
            team_token_ata.key,
            token_mint.key,
            user_ata.key,
            &team_pda,
            &[],
            share,
            OUR_TOKEN_DECIMALS,
        )?;
        
        invoke_signed(
            &transfer_ix,
            &[
                team_token_ata.clone(),
                user_ata.clone(),
                token_mint.clone(),
                token_program.clone(),
            ],
            &[seeds],
        )?;
        
        page_distributed += share;
        processed_count += 1;
        
        msg!("♒~~~🌊🌊🌊~~~:DISTRIBUTED:team={}:rank={}:user={}:amount={}:contrib={}",
            team.immutable.leader, current_rank, member_pubkey, share, user_contrib);
    }
    
    // ==================== ЗАВЕРШЕНИЕ ====================
    // Последняя подстраница последней страницы
    let is_last_chunk = chunk_page + 1 >= team.mutable.num_pages;
    let is_last_sub = end_idx >= members.len();
    
    if is_last_chunk && is_last_sub {
        let mut team_mut = Team::load(team_account)?;
        team_mut.mark_bonus_distributed(current_rank);
        team_mut.mutable.has_pending_distribution = false;
        team_mut.save(team_account)?;
        
        msg!("♒~~~🌊🌊🌊~~~:DISTRIBUTION_COMPLETE:team={}:rank={}",
            team.immutable.leader, current_rank);
    }
    
    msg!("♒~~~🌊🌊🌊~~~:DISTRIBUTE_PAGE:team={}:chunk={}:sub={}:processed={}:amount={}",
        team.immutable.leader, chunk_page, sub_page, processed_count, page_distributed);
    
    Ok(())
}