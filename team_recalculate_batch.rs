// src/instructions/bonus/team_recalculate_batch.rs — v6.0 FINAL
// 🔥 Полный цикл перерасчета и повышения ранга команды
//
// Фаза 1: process_recalculate_batch() — сбор данных по страницам (без комиссии)
// Фаза 2: finalize_recalculation()    — проверка условий ранга (без комиссии)
// Фаза 3: apply_rank_change()         — выплата + снапшот + обновление (с комиссией mode=2)
//
// 🔥 ИЗМЕНЕНИЯ v6.0:
//    - Комиссия mode=2 в apply_rank_change (Team + ATA + Snapshot)
//    - Убран is_recalculating = false (закрывается через sub_tag 254)
//    - Лидер в снапшоте отдельной мессагой SNAPSHOT_LEADER
//    - total_stake_hours НЕ перезаписывается

use crate::{
    constants::*,
    error::WaterError,
    state::{
        team::Team,
        team_chunk::TeamChunk,
        team_tmp_snapshot::TeamTmpSnapshot,
        team_rank_config::TeamRankConfig,
        config_trait::ConfigAccount,
        user::User,
        PlayerStakeData,
    },
    types::TeamRankRequirements,
    utils::validation_utils::{self, validate_root_pda_v2},
    instructions::utilits::pda::{
        derive_team_pda,
        derive_bonus_fund_pda,
        derive_sol_fund_pda,
        get_root_pda_key,
    },
};
use crate::instructions::utils::get_associated_token_address;
use crate::instructions::utils::account_utils::{get_token_balance, get_total_token_balance};

use borsh::BorshDeserialize;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    program::{invoke_signed, invoke},
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar::{rent::Rent, clock::Clock, Sysvar},
    msg,
};
use spl_token_2022_interface::instruction as token_instruction;
use spl_token_2022_interface::state::Mint;
use spl_token_2022_interface::extension::StateWithExtensions;
use solana_system_interface::instruction as system_instruction;

// ==================== ТРАНЗАКЦИЯ 1: БАТЧ-ПЕРЕРАСЧЕТ (БЕЗ КОМИССИИ) ====================

/// Аккаунты:
/// 0. [signer] payer
/// 1. [writable] team_account
/// 2. [writable] snapshot_account
/// 3. [] team_rank_config
/// 4. [] team_chunk_account (страница N)
/// 5. [] token_mint
/// 6. [] system_program
/// ... user_pda + token_ata + stake_data × 19
pub fn process_recalculate_batch(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    page: u8,
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    
    let payer = next_account_info(accounts_iter)?;               // 0
    let team_account = next_account_info(accounts_iter)?;        // 1
    let snapshot_account = next_account_info(accounts_iter)?;    // 2
    let _team_rank_config = next_account_info(accounts_iter)?;   // 3
    let team_chunk_account = next_account_info(accounts_iter)?;  // 4
    let token_mint = next_account_info(accounts_iter)?;          // 5
    let _system_program = next_account_info(accounts_iter)?;     // 6
    
    let remaining_accounts = accounts_iter.as_slice();
    
    // ==================== ПРОВЕРКИ ====================
    if !payer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    
    if team_account.owner != program_id || snapshot_account.owner != program_id {
        return Err(ProgramError::InvalidAccountOwner);
    }
    
    check_mint_decimals(token_mint)?;
    
    let team = Team::load(team_account)?;
    
    if team.immutable.leader != *payer.key {
        return Err(WaterError::Unauthorized.into());
    }
    
    // 🔓 Дверь уже проверена в team_operations.rs
    
    let mut snapshot = TeamTmpSnapshot::load(snapshot_account)?;
    
    if snapshot.team_leader != *payer.key {
        return Err(WaterError::Unauthorized.into());
    }
    
    if snapshot.is_complete {
        return Err(WaterError::SnapshotAlreadyExists.into());
    }
    
    if snapshot.is_rank_paid {
        return Err(WaterError::BonusAlreadyClaimed.into());
    }
    
    if page != snapshot.pages_processed {
        msg!("♒~~~⚡~~~:INVALID_PAGE:expected={}:got={}", snapshot.pages_processed, page);
        return Err(WaterError::InvalidPDA.into());
    }
    
    let (expected_chunk_pda, _) = TeamChunk::find_chunk_pda(
        program_id, &snapshot.team_leader, page
    );
    if team_chunk_account.key != &expected_chunk_pda {
        return Err(WaterError::InvalidPDA.into());
    }
    
    // ==================== ЗАГРУЗКА TEAM CHUNK ====================
    let team_chunk = TeamChunk::load(team_chunk_account)?;
    let members = team_chunk.get_all_members(team_chunk_account)?;
    
    // ==================== СБОР ДАННЫХ ====================
    let mut page_contribution: u64 = 0;
    let mut page_tokens: u64 = 0;
    let mut page_member_count: u32 = 0;
    let mut page_rank_counts = [0u32; MAX_TEAM_RANKS];
    let mut page_leader_contribution: u64 = 0;
    let mut processed_count = 0u32;
    let mut skipped_count = 0u32;
    
    for member_pubkey in &members {
        let (user_pda, _) = User::find_user_address(program_id, member_pubkey);
        let user_account = match remaining_accounts.iter().find(|acc| acc.key == &user_pda) {
            Some(acc) => acc,
            None => { skipped_count += 1; continue; }
        };
        
        let user_ata = get_associated_token_address(member_pubkey, token_mint.key);
        let token_account = match remaining_accounts.iter().find(|acc| acc.key == &user_ata) {
            Some(acc) => acc,
            None => { skipped_count += 1; continue; }
        };
        
        let (stake_pda, _) = PlayerStakeData::find_stake_data_address(program_id, member_pubkey);
        let stake_account = match remaining_accounts.iter().find(|acc| acc.key == &stake_pda) {
            Some(acc) => acc,
            None => { skipped_count += 1; continue; }
        };
        
        let user = match User::get_user(user_account, program_id) {
            Ok(u) => u,
            Err(_) => { skipped_count += 1; continue; }
        };
        
        let total_tokens = get_total_token_balance(token_account, stake_account)?;
        
        page_contribution = page_contribution
            .checked_add(user.team_contribution)
            .ok_or(WaterError::InvalidRewardAmount)?;
        page_tokens = page_tokens
            .checked_add(total_tokens)
            .ok_or(WaterError::InvalidRewardAmount)?;
        page_member_count += 1;
        
        if (user.current_rank as usize) < MAX_TEAM_RANKS {
            page_rank_counts[user.current_rank as usize] += 1;
        }
        
        if *member_pubkey == snapshot.team_leader {
            page_leader_contribution = user.team_contribution;
        }
        
        processed_count += 1;
    }
    
    // ==================== АККУМУЛЯЦИЯ ====================
    snapshot.add_page_data(
        page_contribution, page_tokens, page_member_count,
        &page_rank_counts, page_leader_contribution,
    )?;
    snapshot.save(snapshot_account)?;
    
    msg!("♒~~~🌊🌊🌊~~~:RECALC_PAGE:team={}:page={}:of={}:processed={}:skipped={}:contrib={}:tokens={}",
        snapshot.team_leader, page + 1, snapshot.pages_total,
        processed_count, skipped_count, page_contribution, page_tokens);
    
    Ok(())
}

// ==================== ТРАНЗАКЦИЯ 2: ПРОВЕРКА РАНГА (БЕЗ КОМИССИИ) ====================

/// Аккаунты:
/// 0. [signer] payer
/// 1. [] team_account
/// 2. [] snapshot_account
/// 3. [] team_rank_config
/// 4. [] token_mint
pub fn finalize_recalculation(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    
    let payer = next_account_info(accounts_iter)?;
    let team_account = next_account_info(accounts_iter)?;
    let snapshot_account = next_account_info(accounts_iter)?;
    let team_rank_config_account = next_account_info(accounts_iter)?;
    let token_mint = next_account_info(accounts_iter)?;
    
    if !payer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    
    check_mint_decimals(token_mint)?;
    
    let team = Team::load(team_account)?;
    if team.immutable.leader != *payer.key {
        return Err(WaterError::Unauthorized.into());
    }
    
    let snapshot = TeamTmpSnapshot::load(snapshot_account)?;
    
    if !snapshot.is_complete {
        msg!("♒~~~⚡~~~:RECALC_NOT_COMPLETE:processed={}:total={}",
            snapshot.pages_processed, snapshot.pages_total);
        return Err(WaterError:: SnapshotNotFinalized.into());
    }
    
    if snapshot.is_rank_paid {
        return Err(WaterError::BonusAlreadyClaimed.into());
    }
    
    let team_rank_config = TeamRankConfig::try_from_slice(&team_rank_config_account.data.borrow())
        .map_err(|_| WaterError::SerializationError)?;
    let acc = snapshot.get_accumulated_data();
    let current_rank = team.mutable.current_rank;
    
    // Проверка повышения
    let target_rank = current_rank + 1;
    let mut can_upgrade = false;
    let mut can_downgrade_to: Option<u8> = None;
    
    if (target_rank as usize) < team_rank_config.team_ranks.len() {
        let req = &team_rank_config.team_ranks[target_rank as usize];
        if req.is_active {
            can_upgrade = check_requirements(&acc, req);
        }
    }
    
    // Проверка понижения
    if !can_upgrade && current_rank > 0 {
        let mut check_rank = current_rank;
        while check_rank > 0 {
            let req = &team_rank_config.team_ranks[check_rank as usize];
            if req.is_active && check_requirements(&acc, req) {
                break;
            }
            check_rank -= 1;
        }
        if check_rank < current_rank {
            can_downgrade_to = Some(check_rank);
        }
    }
    
    // Лог результата
    if can_upgrade {
        let reward = team_rank_config.team_rank_rewards[target_rank as usize];
        msg!("♒~~~🌊🌊🌊~~~:RANK_CHECK:team={}:current={}:can_upgrade=true:target={}:reward={}",
            snapshot.team_leader, current_rank, target_rank, reward);
    } else if let Some(downgrade_rank) = can_downgrade_to {
        msg!("♒~~~🌊🌊🌊~~~:RANK_CHECK:team={}:current={}:can_downgrade=true:target={}",
            snapshot.team_leader, current_rank, downgrade_rank);
    } else {
        msg!("♒~~~🌊🌊🌊~~~:RANK_CHECK:team={}:current={}:no_change=true",
            snapshot.team_leader, current_rank);
    }
    
    Ok(())
}

// ==================== ТРАНЗАКЦИЯ 3: ПРИМЕНЕНИЕ РАНГА (С КОМИССИЕЙ mode=2) ====================

/// Аккаунты:
/// 0. [signer] payer
/// 1. [writable] team_account
/// 2. [writable] snapshot_account
/// 3. [] team_rank_config
/// 4. [writable] bonus_fund_ata
/// 5. [writable] team_token_ata
/// 6. [] token_mint
/// 7. [] token_program
/// 8. [] immutable_seed_account
/// 9. [writable] rent_pda          ← комиссия
/// 10. [] system_program           ← комиссия
/// 11. [writable] sol_fund         ← комиссия
/// 12. [] root_pda_account         ← комиссия
/// ... участники для снапшота (user_pda + token_ata + stake_data)
pub fn apply_rank_change(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    new_rank: u8,
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    
    let payer = next_account_info(accounts_iter)?;                  // 0
    let team_account = next_account_info(accounts_iter)?;           // 1
    let snapshot_account = next_account_info(accounts_iter)?;       // 2
    let team_rank_config_account = next_account_info(accounts_iter)?; // 3
    let bonus_fund_ata = next_account_info(accounts_iter)?;         // 4
    let team_token_ata = next_account_info(accounts_iter)?;         // 5
    let token_mint = next_account_info(accounts_iter)?;             // 6
    let token_program = next_account_info(accounts_iter)?;          // 7
    let immutable_seed_account = next_account_info(accounts_iter)?; // 8
    let rent_pda = next_account_info(accounts_iter)?;               // 9
    let system_program = next_account_info(accounts_iter)?;         // 10
    let sol_fund = next_account_info(accounts_iter)?;               // 11
    let root_pda_account = next_account_info(accounts_iter)?;       // 12
    
    let member_accounts = accounts_iter.as_slice();
    
    // ==================== ПРОВЕРКИ ====================
    if !payer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    
    if team_account.owner != program_id || team_rank_config_account.owner != program_id {
        return Err(ProgramError::InvalidAccountOwner);
    }
    
    if *token_program.key != spl_token_2022_interface::ID {
        return Err(ProgramError::IncorrectProgramId);
    }
    
    if rent_pda.owner != program_id {
        return Err(WaterError::IllegalOwner.into());
    }
    
    check_mint_decimals(token_mint)?;
    
    // Проверка SOL Fund
    let root_pda = get_root_pda_key(program_id, root_pda_account)?;
    let (expected_sol_fund, _) = derive_sol_fund_pda(program_id, &root_pda);
    if sol_fund.key != &expected_sol_fund || sol_fund.owner != program_id {
        return Err(WaterError::InvalidPDA.into());
    }
    
    let root_pda_full = validate_root_pda_v2(program_id, immutable_seed_account)?;
    let team_rank_config = TeamRankConfig::get_config(team_rank_config_account, program_id, &root_pda_full)?;
    
    check_team_pdas(
        team_account, token_mint, team_token_ata,
        bonus_fund_ata, &root_pda_full, program_id, payer.key
    )?;
    
    // ==================== КОМИССИЯ (mode=2: Team + ATA + Snapshot) ====================
    let rent = Rent::get()?;
    let team_rent = rent.minimum_balance(Team::LEN);
    let ata_rent = rent.minimum_balance(165);
    let snapshot_rent = rent.minimum_balance(TeamTmpSnapshot::LEN);
    
    let commission = team_rent
        .checked_add(ata_rent)
        .and_then(|v| v.checked_add(snapshot_rent))
        .ok_or(WaterError::InvalidTransfer)?;
    
    msg!("💰 Commission: {} lamports", commission);
    
    if **payer.try_borrow_lamports()? < commission {
        return Err(WaterError::InsufficientBalance.into());
    }
    
    invoke(
        &system_instruction::transfer(payer.key, rent_pda.key, commission),
        &[payer.clone(), rent_pda.clone(), system_program.clone()],
    )?;
    msg!("💰 PAID: {} lamports to shard", commission);
    
    return_surplus(rent_pda, sol_fund, &rent)?;
    
    // ==================== ЗАГРУЗКА ДАННЫХ ====================
    let mut team = Team::load(team_account)?;
    
    let current_time = Clock::get()?.unix_timestamp;
    let old_rank = team.mutable.current_rank;
    
    if team.is_bonus_claimed(new_rank) {
        return Err(WaterError::BonusAlreadyClaimed.into());
    }
    
    let mut snapshot = TeamTmpSnapshot::load(snapshot_account)?;
    
    if !snapshot.is_complete {
        return Err(WaterError::SnapshotAlreadyExists.into());
    }
    
    if snapshot.is_rank_paid {
        return Err(WaterError::BonusAlreadyClaimed.into());
    }
    
    let acc = snapshot.get_accumulated_data();
    
    // ==================== ПОВЫШЕНИЕ ====================
    if new_rank > old_rank {
        let req = &team_rank_config.team_ranks[new_rank as usize];
        let reward_amount = team_rank_config.team_rank_rewards[new_rank as usize];
        
        if !check_requirements(&acc, req) {
            return Err(WaterError::InvalidRank.into());
        }
        
        // Расчет долей
        let team_stake_hours = team.mutable.total_stake_hours;
        let gap = team_stake_hours.saturating_sub(acc.total_contribution);
        let leader_total = acc.leader_contribution.saturating_add(gap);
        
        let leader_tokens = if team_stake_hours > 0 {
            (leader_total as u128 * reward_amount as u128 / team_stake_hours as u128) as u64
        } else { 0 };
        let team_tokens = reward_amount.saturating_sub(leader_tokens);
        
        let leader_pct = if reward_amount > 0 {
            (leader_tokens as u128 * 10000 / reward_amount as u128) as u64
        } else { 0 };
        let team_pct = 10000u64.saturating_sub(leader_pct);
        
        let team_pool_contrib = acc.total_contribution.saturating_sub(acc.leader_contribution);
        
        // Выплата бонуса
        if reward_amount > 0 {
            let (expected_vault, vault_bump) = derive_bonus_fund_pda(program_id, &root_pda_full);
            
            if get_token_balance(bonus_fund_ata)? < reward_amount {
                return Err(WaterError::InsufficientFunds.into());
            }
            
            let transfer_ix = token_instruction::transfer_checked(
                token_program.key, bonus_fund_ata.key, token_mint.key,
                team_token_ata.key, &expected_vault, &[],
                reward_amount, OUR_TOKEN_DECIMALS,
            )?;
            
            let seeds = &[BONUS_FUND_SEED, root_pda_full.as_ref(), &[vault_bump]];
            invoke_signed(&transfer_ix, &[
                bonus_fund_ata.clone(), team_token_ata.clone(),
                token_mint.clone(), token_program.clone(),
            ], &[seeds])?;
            
            msg!("♒~~~🌊🌊🌊~~~:TEAM_PAID:team={}:rank={}:amount={}",
                payer.key, new_rank, reward_amount);
        }
        
        // Снапшот: Критерии
        msg!("♒~~~🌊🌊🌊~~~:RANK_REQUIREMENTS_MET:team={}:target_rank={}:min_tokens={}:actual_tokens={}:min_stake={}:actual_stake={}:min_members={}:actual_members={}:required_ranks={:?}:actual_ranks={:?}",
            payer.key, new_rank,
            req.min_team_tokens, acc.total_tokens,
            req.min_team_stake_hours, acc.total_contribution,
            req.min_members, acc.member_count,
            req.required_members, acc.rank_counts);
        
        // Снапшот: Заголовок
        msg!("♒~~~🌊🌊🌊~~~:SNAPSHOT_HEADER:team={}:rank={}:reward={}:team_stake_hours={}:total_contrib={}:leader_contrib={}:gap={}:leader_pct={}:team_pct={}:leader_tokens={}:team_tokens={}:total_tokens={}:member_count={}",
            payer.key, new_rank, reward_amount,
            team_stake_hours, acc.total_contribution, acc.leader_contribution,
            gap, leader_pct, team_pct, leader_tokens, team_tokens,
            acc.total_tokens, acc.member_count);
        
        // Снапшот: Участники (БЕЗ ЛИДЕРА)
        let mut leader_balance: u64 = 0;
        let mut leader_staked: u64 = 0;
        
        for page in 0..snapshot.pages_total {
            let (chunk_pda, _) = TeamChunk::find_chunk_pda(
                program_id, &snapshot.team_leader, page
            );
            
            if let Some(chunk_acc) = member_accounts.iter().find(|acc| acc.key == &chunk_pda) {
                if let Ok(chunk) = TeamChunk::load(chunk_acc) {
                    if let Ok(members) = chunk.get_all_members(chunk_acc) {
                        for member_pubkey in &members {
                            if *member_pubkey == snapshot.team_leader {
                                let user_ata = get_associated_token_address(member_pubkey, token_mint.key);
                                if let Some(ata) = member_accounts.iter().find(|acc| acc.key == &user_ata) {
                                    leader_balance = get_token_balance(ata).unwrap_or(0);
                                }
                                let (stake_pda, _) = PlayerStakeData::find_stake_data_address(program_id, member_pubkey);
                                if let Some(sa) = member_accounts.iter().find(|acc| acc.key == &stake_pda) {
                                    leader_staked = get_staked_amount(sa).unwrap_or(0);
                                }
                                continue;
                            }
                            
                            let (user_pda, _) = User::find_user_address(program_id, member_pubkey);
                            let user_contrib = member_accounts.iter()
                                .find(|acc| acc.key == &user_pda)
                                .and_then(|ua| User::get_user(ua, program_id).ok())
                                .map(|u| u.team_contribution)
                                .unwrap_or(0);
                            
                            let user_ata = get_associated_token_address(member_pubkey, token_mint.key);
                            let balance = member_accounts.iter()
                                .find(|acc| acc.key == &user_ata)
                                .and_then(|ata| get_token_balance(ata).ok())
                                .unwrap_or(0);
                            
                            let (stake_pda, _) = PlayerStakeData::find_stake_data_address(program_id, member_pubkey);
                            let staked = member_accounts.iter()
                                .find(|acc| acc.key == &stake_pda)
                                .and_then(|sa| get_staked_amount(sa).ok())
                                .unwrap_or(0);
                            
                            let user_share = if team_pool_contrib > 0 {
                                (user_contrib as u128 * team_tokens as u128 / team_pool_contrib as u128) as u64
                            } else { 0 };
                            
                            msg!("♒~~~🌊🌊🌊~~~:SNAPSHOT_MEMBER:team={}:rank={}:user={}:contrib={}:balance={}:bonus_tokens={}",
                                payer.key, new_rank, member_pubkey,
                                user_contrib, balance + staked, user_share);
                        }
                    }
                }
            }
        }
        
        // Лидер отдельно
        msg!("♒~~~🌊🌊🌊~~~:SNAPSHOT_LEADER:team={}:rank={}:user={}:is_leader=true:contrib={}:balance={}:bonus_tokens={}",
            payer.key, new_rank, snapshot.team_leader,
            acc.leader_contribution, leader_balance + leader_staked, leader_tokens);
        
        // Обновление Team
        team.mutable.current_rank = new_rank;
        team.mutable.rank_bonuses_claimed[new_rank as usize] = true;
        team.mutable.rank_bonuses_distributed[new_rank as usize] = false;
        team.mutable.has_pending_distribution = true;
        team.mutable.rank_epoch = team.mutable.rank_epoch.wrapping_add(1);
        team.mutable.last_rank_update = current_time;
        team.mutable.team_total_tokens = acc.total_tokens;
        team.mutable.member_count = acc.member_count;
        team.mutable.rank_member_counts = acc.rank_counts;
        
        team.save(team_account)?;
        
        // 🔥 Сохраняем доли для дистрибуции
        snapshot.mark_paid_with_shares(leader_tokens, team_tokens);
        snapshot.save(snapshot_account)?;
        
        msg!("♒~~~🌊🌊🌊~~~:RANK_UP:team={}:old={}:new={}:epoch={}",
            payer.key, old_rank, new_rank, team.mutable.rank_epoch);
    }
    // ==================== ПОНИЖЕНИЕ ====================
    else if new_rank < old_rank {
        team.mutable.current_rank = new_rank;
        team.mutable.last_rank_update = current_time;
        team.mutable.team_total_tokens = acc.total_tokens;
        team.mutable.member_count = acc.member_count;
        team.mutable.rank_member_counts = acc.rank_counts;
        
        team.save(team_account)?;
        
        snapshot.mark_paid();
        snapshot.save(snapshot_account)?;
        
        msg!("♒~~~🌊🌊🌊~~~:RANK_DOWN:team={}:old={}:new={}",
            payer.key, old_rank, new_rank);
    }
    // ==================== БЕЗ ИЗМЕНЕНИЙ ====================
    else {
        team.save(team_account)?;
        snapshot.save(snapshot_account)?;
        
        msg!("♒~~~🌊🌊🌊~~~:RANK_NO_CHANGE:team={}:rank={}",
            payer.key, old_rank);
    }
    
    // 🔥 НЕ закрываем дверь! (закрывается через sub_tag 254)
    
    msg!("♒~~~🌊🌊🌊~~~:RECALC_DONE:team={}:rank={}",
        payer.key, team.mutable.current_rank);
    
    Ok(())
}

// ==================== ВСПОМОГАТЕЛЬНЫЕ ФУНКЦИИ ====================

fn check_requirements(
    acc: &crate::state::team_tmp_snapshot::RecalcAccumulatedData,
    req: &TeamRankRequirements,
) -> bool {
    if acc.total_tokens < req.min_team_tokens { return false; }
    if acc.total_contribution < req.min_team_stake_hours { return false; }
    if acc.member_count < req.min_members as u32 { return false; }
    
    for i in 0..MAX_TEAM_RANKS {
        if req.required_members[i] > 0 && acc.rank_counts[i] < req.required_members[i] as u32 {
            return false;
        }
    }
    true
}

fn check_mint_decimals(token_mint: &AccountInfo) -> Result<(), ProgramError> {
    let mint_data = token_mint.data.borrow();
    let mint = StateWithExtensions::<Mint>::unpack(&mint_data)
        .map_err(|_| WaterError::InvalidAccountData)?;
    if mint.base.decimals != OUR_TOKEN_DECIMALS {
        return Err(WaterError::InvalidDecimals.into());
    }
    Ok(())
}

fn check_team_pdas(
    team_account: &AccountInfo,
    token_mint: &AccountInfo,
    team_token_ata: &AccountInfo,
    team_bonus_vault: &AccountInfo,
    root_pda: &Pubkey,
    program_id: &Pubkey,
    leader: &Pubkey,
) -> Result<(), ProgramError> {
    let expected_team_ata = get_associated_token_address(team_account.key, token_mint.key);
    if team_token_ata.key != &expected_team_ata {
        return Err(WaterError::InvalidPDA.into());
    }

    let (expected_vault, _) = derive_bonus_fund_pda(program_id, root_pda);
    let expected_bonus_ata = get_associated_token_address(&expected_vault, token_mint.key);
    if team_bonus_vault.key != &expected_bonus_ata {
        return Err(WaterError::InvalidPDA.into());
    }

    let (expected_team_pda, _) = derive_team_pda(program_id, leader);
    if team_account.key != &expected_team_pda {
        return Err(WaterError::InvalidPDA.into());
    }

    Ok(())
}

fn get_staked_amount(stake_data_account: &AccountInfo) -> Result<u64, ProgramError> {
    let stake_data = PlayerStakeData::try_from_slice(&stake_data_account.data.borrow())
        .map_err(|_| ProgramError::from(WaterError::SerializationError))?;
    Ok(stake_data.active_stakes.iter().map(|s| s.amount).sum())
}

fn return_surplus(rent_pda: &AccountInfo, sol_fund: &AccountInfo, rent: &Rent) -> ProgramResult {
    let shard_balance = **rent_pda.try_borrow_lamports()?;
    let min_shard = rent.minimum_balance(rent_pda.data_len());
    let target = min_shard.checked_mul(5).ok_or(WaterError::InvalidTransfer)?;
    
    if shard_balance > target {
        let surplus = shard_balance - target;
        **rent_pda.try_borrow_mut_lamports()? = target;
        **sol_fund.try_borrow_mut_lamports()? = sol_fund.lamports()
            .checked_add(surplus)
            .ok_or(WaterError::InvalidTransfer)?;
        msg!("💰 Surplus {} lamports returned to SOL Fund", surplus);
    }
    
    Ok(())
}