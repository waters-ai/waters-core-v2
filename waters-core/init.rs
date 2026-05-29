// src/instructions/bonus/init.rs — v1.0
// 🔥 Инициализация перерасчета с комиссией
// sub_tag 0: PAY_AND_INIT

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

use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar::{rent::Rent, clock::Clock, Sysvar},
    msg,
};
use solana_system_interface::instruction as system_instruction;

/// Аккаунты:
/// 0. [signer] payer — любой член команды
/// 1. [writable] team_account — Team PDA
/// 2. [writable] snapshot_account — TeamTmpSnapshot PDA (создается здесь)
/// 3. [] team_chunk_account — TeamChunk (страница 0, для проверки)
/// 4. [] token_mint
/// 5. [writable] rent_pda — шард плательщика
/// 6. [] system_program
/// 7. [writable] sol_fund — SOL Fund PDA
/// 8. [] root_pda_account — Root PDA
/// 9. [] team_ata_account — ATA команды (для расчета комиссии)
pub fn process_init_recalculation(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    
    let payer = next_account_info(accounts_iter)?;              // 0
    let team_account = next_account_info(accounts_iter)?;       // 1
    let snapshot_account = next_account_info(accounts_iter)?;   // 2
    let _team_chunk = next_account_info(accounts_iter)?;        // 3
    let _token_mint = next_account_info(accounts_iter)?;        // 4
    let rent_pda = next_account_info(accounts_iter)?;           // 5
    let system_program = next_account_info(accounts_iter)?;     // 6
    let sol_fund = next_account_info(accounts_iter)?;           // 7
    let root_pda_account = next_account_info(accounts_iter)?;   // 8
    let team_ata_account = next_account_info(accounts_iter)?;   // 9
    
    // ==================== ПРОВЕРКИ ====================
    if !payer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    
    if team_account.owner != program_id {
        return Err(ProgramError::InvalidAccountOwner);
    }
    if rent_pda.owner != program_id {
        return Err(WaterError::IllegalOwner.into());
    }
    
    let root_pda = get_root_pda_key(program_id, root_pda_account)?;
    let (expected_sol_fund, _) = derive_sol_fund_pda(program_id, &root_pda);
    if sol_fund.key != &expected_sol_fund || sol_fund.owner != program_id {
        return Err(WaterError::InvalidPDA.into());
    }
    
    let mut team = Team::load(team_account)?;
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
    
    // ==================== КОМИССИЯ ====================
    let rent = Rent::get()?;
    let team_rent = rent.minimum_balance(Team::LEN);
    let ata_rent = rent.minimum_balance(165);
    let snapshot_rent = rent.minimum_balance(TeamTmpSnapshot::LEN);
    
    let commission = team_rent
        .checked_add(ata_rent)
        .and_then(|v| v.checked_add(snapshot_rent))
        .ok_or(WaterError::InvalidTransfer)?;
    
    msg!("💰 Commission: {} lamports (Team={} + ATA={} + Snapshot={})",
        commission, team_rent, ata_rent, snapshot_rent);
    
    if **payer.try_borrow_lamports()? < commission {
        return Err(WaterError::InsufficientBalance.into());
    }
    
    invoke(
        &system_instruction::transfer(payer.key, rent_pda.key, commission),
        &[payer.clone(), rent_pda.clone(), system_program.clone()],
    )?;
    msg!("💰 PAID: {} lamports to shard", commission);
    
    // ==================== ВОЗВРАТ ИЗЛИШКОВ В SOL FUND ====================
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
    
    // ==================== СОЗДАНИЕ SNAPSHOT PDA ====================
    let target_rank = team.mutable.current_rank + 1;
    let (snapshot_pda, snapshot_bump) = TeamTmpSnapshot::find_pda(
        program_id, &team.immutable.leader, target_rank
    );
    
    if snapshot_account.key != &snapshot_pda {
        msg!("❌ Wrong snapshot PDA: expected {}", snapshot_pda);
        return Err(WaterError::InvalidPDA.into());
    }
    
    let seeds = &[
        b"team_snapshot",
        team.immutable.leader.as_ref(),
        &[target_rank],
        &[snapshot_bump],
    ];
    
    // Выделяем память
    invoke_signed(
        &system_instruction::allocate(snapshot_account.key, TeamTmpSnapshot::LEN as u64),
        &[snapshot_account.clone(), system_program.clone()],
        &[seeds],
    )?;
    
    // Назначаем владельца
    invoke_signed(
        &system_instruction::assign(snapshot_account.key, program_id),
        &[snapshot_account.clone(), system_program.clone()],
        &[seeds],
    )?;
    
    // Аренда из шарда
    **rent_pda.try_borrow_mut_lamports()? -= snapshot_rent;
    **snapshot_account.try_borrow_mut_lamports()? += snapshot_rent;
    
    // Инициализируем данные снапшота
    let snapshot = TeamTmpSnapshot::new(
        team.immutable.leader,
        target_rank,
        team.mutable.num_pages,
        snapshot_bump,
    );
    snapshot.save(snapshot_account)?;
    
    // ==================== 🔓 ОТКРЫВАЕМ ДВЕРЬ ====================
    team.mutable.is_recalculating = true;
    team.mutable.last_recalculation_time = Clock::get()?.unix_timestamp;
    team.save(team_account)?;
    
    msg!("🔓 DOOR OPENED: team={}, target_rank={}, pages={}",
        team.immutable.leader, target_rank, team.mutable.num_pages);
    msg!("✅ INIT DONE");
    
    Ok(())
}