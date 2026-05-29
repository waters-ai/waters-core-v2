// src/instructions/bonus/team_operations.rs — v2.0
// 🔥 ЕДИНЫЙ ВХОД ДЛЯ ВСЕХ ОПЕРАЦИЙ С КОМАНДОЙ (тег 7)
// Только проверка двери + маршрутизация

use crate::{
    error::WaterError,
    state::team::Team,
};
use solana_program::{
    account_info::AccountInfo,
    entrypoint::ProgramResult,
    pubkey::Pubkey,
    msg,
};

pub fn process_team_operations(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    if data.is_empty() {
        return Err(solana_program::program_error::ProgramError::InvalidInstructionData.into());
    }
    msg!("data[0]={} data[1]={}", data[0], data[1]);
    let sub_tag = data[0];
    let rest = &data[1..];
    
    match sub_tag {
        0 => {
            msg!("🔓 TEAM_OP: INIT");
            let team = Team::load(&accounts[1])?;
            if team.mutable.is_recalculating {
                return Err(WaterError::SnapshotAlreadyExists.into());
            }
            super::init::process_init_recalculation(program_id, accounts)
        }
        
        1..=250 => {
            let page = sub_tag - 1;
            msg!("📊 TEAM_OP: BATCH page={}", page);
            let team = Team::load(&accounts[1])?;
            if !team.mutable.is_recalculating {
                return Err(WaterError::SnapshotNotFinalized.into());
            }
            super::team_recalculate_batch::process_recalculate_batch(program_id, accounts, page)
        }
        
        251 => {
            msg!("🔍 TEAM_OP: FINALIZE");
            let team = Team::load(&accounts[1])?;
            if !team.mutable.is_recalculating {
                return Err(WaterError::SnapshotNotFinalized.into());
            }
            super::team_recalculate_batch::finalize_recalculation(program_id, accounts)
        }
        
        252 => {
            let new_rank = rest.first().copied().unwrap_or(0);
            msg!("💰 TEAM_OP: APPLY_RANK new_rank={}", new_rank);
            let team = Team::load(&accounts[1])?;
            if !team.mutable.is_recalculating {
                return Err(WaterError::InvalidRank.into());
            }
            super::team_recalculate_batch::apply_rank_change(program_id, accounts, new_rank)
        }
        
        253 => {
            let chunk_page = rest.first().copied().unwrap_or(0);
            let sub_page = rest.get(1).copied().unwrap_or(0);
            msg!("💸 TEAM_OP: DISTRIBUTE chunk={}, sub={}", chunk_page, sub_page);
            super::distribute_page::process_distribute_page(program_id, accounts, chunk_page, sub_page)
        },
        
        254 => {
            msg!("🔒 TEAM_OP: CLOSE");
            close_door(accounts)
        }
        
        _ => Err(solana_program::program_error::ProgramError::InvalidInstructionData.into()),
    }
}

fn close_door(accounts: &[AccountInfo]) -> ProgramResult {
    let team_account = &accounts[1];
    let mut team = Team::load(team_account)?;
    team.mutable.is_recalculating = false;
    team.save(team_account)?;
    msg!("🔒 DOOR CLOSED");
    Ok(())
}