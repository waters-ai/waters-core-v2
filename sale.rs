// src/instructions/sale/sale.rs — v3.1 FINAL (ВСЕ проверки + SPL Token + PDA автовывод)
// 🔥 ПОЛНАЯ ВЕРСИЯ — всё из старого кода восстановлено + новый функционал
use borsh::BorshSerialize;
use crate::{
    constants::*,
    error::WaterError,
    instructions::user::check_account_owner,
    instructions::utils::get_associated_token_address,
    instructions::utils::account_utils::get_token_balance,
    params::sale::PurchaseTokensParams,
    state::program_config::ProgramConfig,
    state::config_trait::ConfigAccount,
    state::sale::SaleConfig,
    state::user::User,
};
use solana_program::{
    program::invoke_signed,
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar::clock::Clock,
    sysvar::Sysvar,
};
use spl_token_2022_interface::{
    extension::StateWithExtensions,
    state::Mint as Token2022Mint,
    instruction as token_instruction,
};
use crate::instructions::utilits::pda::{get_root_pda_key, derive_config_pda_3, derive_token_fund_pda};

/// Обработчик инструкции покупки токенов
/// 
/// Аккаунты:
/// 0. [signer] user_wallet
/// 1. [writable] user_account (User PDA)
/// 2. [writable] sale_config_account
/// 3. [] token_mint_account (WATER Mint)
/// 4. [writable] usdt_token_account (ATA пользователя для WUSD)
/// 5. [] stablecoin_mint (WUSD Mint)
/// 6. [] fee_wallet (внешний кошелёк комиссии)
/// 7. [writable] fee_wallet_usdt_ata
/// 8. [writable] treasury_pda
/// 9. [writable] treasury_usdt_ata
/// 10. [] space_wallet (внешний кошелёк создателя) 🆕
/// 11. [writable] space_wallet_usdt_ata 🆕
/// 12. [] token_fund_pda
/// 13. [writable] token_fund_ata
/// 14. [writable] user_project_token_account (ATA пользователя для WATER)
/// 15. [] token_program
/// 16. [] associated_token_program
/// 17. [] root_pda_account
/// 18. [] program_config_account
pub fn process_buy_tokens_by_usd(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    params: PurchaseTokensParams,
) -> ProgramResult {
    let account_iter = &mut accounts.iter();

    // ========== ЧТЕНИЕ АККАУНТОВ ==========
    let user_wallet = next_account_info(account_iter)?;               // 0
    let user_account = next_account_info(account_iter)?;              // 1
    let sale_config_account = next_account_info(account_iter)?;       // 2
    let token_mint_account = next_account_info(account_iter)?;        // 3 WATER Mint
    let usdt_token_account = next_account_info(account_iter)?;        // 4 ATA пользователя WUSD
    let stablecoin_mint = next_account_info(account_iter)?;           // 5 WUSD Mint
    let fee_wallet = next_account_info(account_iter)?;                // 6
    let fee_wallet_usdt_ata = next_account_info(account_iter)?;       // 7
    let treasury_pda = next_account_info(account_iter)?;              // 8
    let treasury_usdt_ata = next_account_info(account_iter)?;         // 9
    let space_wallet = next_account_info(account_iter)?;              // 10 🆕
    let space_wallet_usdt_ata = next_account_info(account_iter)?;     // 11 🆕
    let token_fund_pda = next_account_info(account_iter)?;            // 12
    let token_fund_ata = next_account_info(account_iter)?;            // 13
    let user_project_token_account = next_account_info(account_iter)?; // 14 ATA пользователя WATER
    let token_program = next_account_info(account_iter)?;             // 15
    let associated_token_program = next_account_info(account_iter)?;  // 16
    let root_pda_account = next_account_info(account_iter)?;          // 17
    let program_config_account = next_account_info(account_iter)?;    // 18

    // ========== ПРОВЕРКА ПОДПИСАНТА ==========
    if !user_wallet.is_signer {
        msg!("❌ user_wallet должен быть подписантом!");
        return Err(WaterError::Unauthorized.into());
    }

    // ========== ПОЛУЧАЕМ ROOT PDA ==========
    let root_pda = get_root_pda_key(program_id, root_pda_account)?;

    // ========== ДЕСЕРИАЛИЗУЕМ КОНФИГИ (ЗАГРУЖАЕМ ДО ПРОВЕРОК!) ==========
    let program_config = ProgramConfig::get_config(program_config_account, program_id, &root_pda)?;
    let mut sale_config = SaleConfig::get_config(sale_config_account, program_id, &root_pda)?;

    // ========== ПРОВЕРКА ПРОГРАММ ==========
    if token_program.key != &spl_token_2022_interface::id() 
        && token_program.key != &sale_config.stablecoin_program {
        msg!("❌ Неверная токен-программа!");
        msg!("   Ожидалась: Token-2022 или {}", sale_config.stablecoin_program);
        return Err(WaterError::InvalidTokenProgram.into());
    }
    if associated_token_program.key != &ASSOCIATED_TOKEN_PROGRAM_ID {
        msg!("❌ Неверная ассоциированная токен-программа!");
        return Err(WaterError::InvalidTokenProgram.into());
    }

    // ========== ПРОВЕРКИ ВЛАДЕЛЬЦЕВ ATA ==========
    // WATER (Token-2022):
    check_account_owner(token_mint_account, &spl_token_2022_interface::id())?;
    check_account_owner(user_project_token_account, &spl_token_2022_interface::id())?;
    check_account_owner(token_fund_ata, &spl_token_2022_interface::id())?;

    // WUSD — проверяем через sale_config.stablecoin_program:
    check_account_owner(usdt_token_account, &sale_config.stablecoin_program)?;
    check_account_owner(fee_wallet_usdt_ata, &sale_config.stablecoin_program)?;
    check_account_owner(treasury_usdt_ata, &sale_config.stablecoin_program)?;
    check_account_owner(space_wallet_usdt_ata, &sale_config.stablecoin_program)?;
    check_account_owner(stablecoin_mint, &sale_config.stablecoin_program)?;

    // ========== ПРОВЕРКИ БИЗНЕС-ЛОГИКИ ==========
    if !sale_config.has_active_stage() {
        msg!("❌ Нет активной стадии продаж!");
        return Err(WaterError::InvalidSaleStage.into());
    }

    if stablecoin_mint.key != &sale_config.stablecoin_mint {
        msg!("❌ Неверный mint стабильной монеты!");
        msg!("   Ожидался: {}", sale_config.stablecoin_mint);
        msg!("   Получен: {}", stablecoin_mint.key);
        return Err(WaterError::InvalidMint.into());
    }

    if !sale_config.is_active {
        msg!("❌ Продажа не активна!");
        return Err(WaterError::InvalidSaleStage.into());
    }

    if Clock::get()?.unix_timestamp < sale_config.start_time {
        msg!("❌ Продажа ещё не началась!");
        msg!("   Сейчас: {}, Старт: {}", Clock::get()?.unix_timestamp, sale_config.start_time);
        return Err(WaterError::InvalidSaleStage.into());
    }

    // ========== ПРОВЕРКА СТАДИИ И ЦЕНЫ ==========
    let current_stage_idx = sale_config.current_stage as usize;
    if current_stage_idx >= sale_config.stages.len() {
        msg!("❌ Неверный индекс стадии: {}!", current_stage_idx);
        return Err(WaterError::InvalidSaleStage.into());
    }
    let stage = &sale_config.stages[current_stage_idx];

    if stage.price_per_token == 0 {
        msg!("❌ Цена токена не может быть 0!");
        return Err(WaterError::InvalidPrice.into());
    }

    // ========== КОНСТАНТЫ DECIMALS ==========
    const TOKEN_DECIMALS: u32 = 9;
    const USDT_DECIMALS: u32 = 6;
    let token_multiplier = 10u64.checked_pow(TOKEN_DECIMALS).ok_or(WaterError::MathOverflow)?;
    let usdt_multiplier = 10u64.checked_pow(USDT_DECIMALS).ok_or(WaterError::MathOverflow)?;

    // ========== РАСЧЁТ СУММ ==========
    let total_amount = params.usdc_amount;
    
    if total_amount < MIN_DEPOSIT_SALE {
        msg!("❌ Итоговая сумма {} lamports ({} USDC) меньше минимальной {} lamports ({} USDC)",
            total_amount, total_amount / usdt_multiplier,
            MIN_DEPOSIT_SALE, MIN_DEPOSIT_SALE / usdt_multiplier);
        return Err(WaterError::DepositBelowMinimum.into());
    }

    const PERCENT_DENOMINATOR: u64 = 101;
    const PERCENT_NUMERATOR: u64 = 100;
    
    let purchase_amount = total_amount
        .checked_mul(PERCENT_NUMERATOR).ok_or(WaterError::MathOverflow)?
        .checked_div(PERCENT_DENOMINATOR).ok_or(WaterError::MathOverflow)?;
    let fee_amount = total_amount.checked_sub(purchase_amount).ok_or(WaterError::MathOverflow)?;

    msg!("💰 Расчет комиссии (от итоговой суммы):");
    msg!("   ИТОГО получено от пользователя: {} ({} USDC)", total_amount, total_amount / usdt_multiplier);
    msg!("   Сумма покупки (без комиссии): {} ({} USDC)", purchase_amount, purchase_amount / usdt_multiplier);
    msg!("   Комиссия 1%: {} ({} USDC)", fee_amount, fee_amount / usdt_multiplier);

    // ========== ПРОВЕРКА БАЛАНСА WUSD ПОЛЬЗОВАТЕЛЯ ==========
    let user_usdc_balance = get_token_balance(usdt_token_account)?;
    if user_usdc_balance < total_amount {
        msg!("❌ Недостаточно USDC: баланс {} USDC, требуется {} USDC",
            user_usdc_balance / usdt_multiplier, total_amount / usdt_multiplier);
        return Err(WaterError::InsufficientFunds.into());
    }

    // ========== ПРОВЕРКА ATA ПОЛЬЗОВАТЕЛЯ ДЛЯ WUSD ==========
    let expected_user_usdt_ata = get_associated_token_address(user_wallet.key, stablecoin_mint.key);
    if usdt_token_account.key != &expected_user_usdt_ata {
        msg!("❌ Неверный USDT ATA пользователя!");
        msg!("   Ожидался: {}", expected_user_usdt_ata);
        msg!("   Получен: {}", usdt_token_account.key);
        return Err(WaterError::InvalidAccountData.into());
    }

    // ========== ПРОВЕРКА PDA ПОЛЬЗОВАТЕЛЯ ==========
    let (expected_user_pda, _user_pda_bump) =
        Pubkey::find_program_address(&[b"user", user_wallet.key.as_ref()], program_id);
    if user_account.key != &expected_user_pda {
        msg!("❌ PDA пользователя не совпадает!");
        msg!("   Ожидался: {}", expected_user_pda);
        msg!("   Получен: {}", user_account.key);
        return Err(WaterError::InvalidPDA.into());
    }

    if user_account.owner != program_id {
        msg!("❌ PDA пользователя принадлежит другой программе!");
        return Err(ProgramError::IllegalOwner);
    }

    if user_account.data_is_empty() {
        msg!("❌ PDA пользователя не инициализирован!");
        return Err(WaterError::UserAccountNotFound.into());
    }

    let _user = User::get_user(user_account, program_id)?;

    // ========== ПРОВЕРКА ATA ПОЛЬЗОВАТЕЛЯ ДЛЯ WATER ==========
    let expected_user_token_ata = get_associated_token_address(user_account.key, token_mint_account.key);
    
    if user_project_token_account.key == user_account.key {
        msg!("❌ КРИТИЧЕСКАЯ ОШИБКА: Передан PDA вместо ATA!");
        return Err(WaterError::InvalidAccountData.into());
    }
    
    if user_project_token_account.key == user_wallet.key {
        msg!("❌ КРИТИЧЕСКАЯ ОШИБКА: Передан кошелек вместо ATA!");
        return Err(WaterError::InvalidAccountData.into());
    }
    
    if user_project_token_account.key != &expected_user_token_ata {
        msg!("❌ Неверный ATA для токенов проекта!");
        msg!("   Ожидался (от PDA): {}", expected_user_token_ata);
        msg!("   Получен: {}", user_project_token_account.key);
        return Err(WaterError::InvalidAccountData.into());
    }

    // ========== ПРОВЕРКА ВНЕШНИХ КОШЕЛЬКОВ ИЗ PROGRAM_CONFIG ==========
    if fee_wallet.key != &program_config.fee_sale {
        msg!("❌ Неверный fee_sale кошелек!");
        msg!("   Ожидаемый: {}", program_config.fee_sale);
        msg!("   Получен: {}", fee_wallet.key);
        return Err(WaterError::InvalidAccountData.into());
    }

    if space_wallet.key != &program_config.space_wallet {
        msg!("❌ Неверный space_wallet кошелёк!");
        msg!("   Ожидаемый: {}", program_config.space_wallet);
        msg!("   Получен: {}", space_wallet.key);
        return Err(WaterError::InvalidAccountData.into());
    }

    // ========== ПРОВЕРКА ATA ВНЕШНИХ КОШЕЛЬКОВ ==========
    let expected_fee_ata = get_associated_token_address(&program_config.fee_sale, stablecoin_mint.key);
    if fee_wallet_usdt_ata.key != &expected_fee_ata {
        msg!("❌ Неверный ATA для fee_sale");
        msg!("   Ожидался: {}", expected_fee_ata);
        msg!("   Получен: {}", fee_wallet_usdt_ata.key);
        return Err(WaterError::InvalidAccountData.into());
    }

    let expected_space_ata = get_associated_token_address(&program_config.space_wallet, stablecoin_mint.key);
    if space_wallet_usdt_ata.key != &expected_space_ata {
        msg!("❌ Неверный ATA для space_wallet");
        msg!("   Ожидался: {}", expected_space_ata);
        msg!("   Получен: {}", space_wallet_usdt_ata.key);
        return Err(WaterError::InvalidAccountData.into());
    }

    // ========== ПРОВЕРКА ATA TREASURY ==========
    let expected_treasury_ata = get_associated_token_address(treasury_pda.key, stablecoin_mint.key);
    if treasury_usdt_ata.key != &expected_treasury_ata {
        msg!("❌ Неверный ATA для treasury");
        msg!("   Ожидался: {}", expected_treasury_ata);
        msg!("   Получен: {}", treasury_usdt_ata.key);
        return Err(WaterError::InvalidAccountData.into());
    }

    // ========== ПРОВЕРКА TOKEN_FUND ==========
    let (expected_token_fund, _expected_token_fund_bump) = 
        derive_config_pda_3(program_id, TOKEN_FUND_SEED, &root_pda)?;
    
    if token_fund_pda.key != &expected_token_fund {
        msg!("❌ НЕПРАВИЛЬНЫЙ TOKEN_FUND_PDA!");
        msg!("   Ожидался: {}", expected_token_fund);
        msg!("   Получен: {}", token_fund_pda.key);
        return Err(WaterError::InvalidPDA.into());
    }
    
    if token_fund_pda.owner != program_id {
        msg!("❌ TOKEN_FUND_PDA принадлежит другой программе!");
        return Err(ProgramError::IllegalOwner);
    }

    let expected_token_fund_ata = get_associated_token_address(token_fund_pda.key, token_mint_account.key);
    
    if token_fund_ata.key != &expected_token_fund_ata {
        msg!("❌ НЕПРАВИЛЬНЫЙ TOKEN_FUND_ATA!");
        msg!("   Ожидался: {}", expected_token_fund_ata);
        msg!("   Получен: {}", token_fund_ata.key);
        return Err(WaterError::InvalidAccountData.into());
    }
    
    if token_fund_ata.owner != token_program.key {
        msg!("❌ TOKEN_FUND_ATA не принадлежит токен-программе!");
        return Err(WaterError::InvalidAccountData.into());
    }

    // ========== ПРОВЕРКА MINT WATER ==========
    let mint_data = token_mint_account.try_borrow_data()?;
    let mint = StateWithExtensions::<Token2022Mint>::unpack(&mint_data)
        .map_err(|_| WaterError::InvalidAccountData)?;
    let mint_decimals = mint.base.decimals;
    
    if u32::from(mint_decimals) != TOKEN_DECIMALS {
        msg!("❌ НЕСООТВЕТСТВИЕ DECIMALS!");
        msg!("   mint decimals: {} (фактическое)", mint_decimals);
        msg!("   константа token_decimals: {} (ожидаемое)", TOKEN_DECIMALS);
        return Err(WaterError::InvalidAccountData.into());
    }

    // ========== РАСЧЕТ КОЛИЧЕСТВА ТОКЕНОВ WATER ==========
    let amount_of_tokens = purchase_amount
        .checked_mul(token_multiplier).ok_or(WaterError::MathOverflow)?
        .checked_div(stage.price_per_token).ok_or(WaterError::MathOverflow)?;

    if amount_of_tokens == 0 {
        msg!("❌ Слишком маленькая сумма для покупки (0 токенов)");
        return Err(WaterError::InvalidMinDeposit.into());
    }

    let token_lamports = amount_of_tokens;

    // ========== ПРОВЕРКА НАЛИЧИЯ ТОКЕНОВ В КОНФИГЕ ==========
    if amount_of_tokens > sale_config.remaining_tokens {
        msg!("❌ Недостаточно токенов: запрошено {}, доступно {}",
            amount_of_tokens, sale_config.remaining_tokens);
        return Err(WaterError::NoTokensAvailable.into());
    }

    // ========== ПРОВЕРКА БАЛАНСА ФОНДА ==========
    let fund_balance = get_token_balance(token_fund_ata)?;
    
    if fund_balance == 0 {
        msg!("❌ ТОКЕН-ФОНД ПУСТ!");
        return Err(WaterError::InsufficientFunds.into());
    }
    
    if fund_balance < token_lamports {
        msg!("❌ НЕДОСТАТОЧНО ТОКЕНОВ В ФОНДЕ!");
        msg!("   Баланс: {}, требуется: {}", fund_balance, token_lamports);
        return Err(WaterError::InsufficientFunds.into());
    }

    // ========== ПРОВЕРКА ЧТО ФОНД НЕ ПЕРЕВОДИТ САМ СЕБЕ ==========
    if token_fund_ata.key == user_project_token_account.key {
        msg!("❌ Фонд не может переводить токены сам себе!");
        return Err(WaterError::InvalidTransfer.into());
    }

    // ========== СОХРАНЯЕМ СНАПШОТ СОСТОЯНИЯ ==========
    let _old_remaining = sale_config.remaining_tokens;
    let _old_sold = sale_config.sold_tokens;
    let _old_fees = sale_config.total_fees_collected;
    let _old_pda = sale_config.pda_balance;

    // ========== ОБНОВЛЯЕМ СОСТОЯНИЕ ==========
    sale_config.remaining_tokens = sale_config.remaining_tokens
        .checked_sub(amount_of_tokens).ok_or(WaterError::MathOverflow)?;
    
    sale_config.sold_tokens = sale_config.sold_tokens
        .checked_add(amount_of_tokens).ok_or(WaterError::MathOverflow)?;
    
    sale_config.total_fees_collected = sale_config.total_fees_collected
        .checked_add(fee_amount).ok_or(WaterError::MathOverflow)?;
    
    sale_config.pda_balance = sale_config.pda_balance
        .checked_add(purchase_amount).ok_or(WaterError::MathOverflow)?;
    
    sale_config.last_updated = Clock::get()?.unix_timestamp;

    // 🔥 АВТОВЫВОД ИЗЛИШКОВ НА SPACE_WALLET (PDA подписывает через invoke_signed)
    sale_config.check_and_withdraw_excess(
        treasury_pda, space_wallet, stablecoin_mint,
        program_config_account, program_id, &root_pda,
    )?;

    // ========== СЕРИАЛИЗУЕМ ОБНОВЛЕННЫЙ SALE_CONFIG ==========
    sale_config.serialize(&mut *sale_config_account.data.borrow_mut())
        .map_err(|_| WaterError::SerializationError)?;

    // ========== ПЕРЕВОДЫ WUSD (пользователь подписывает) ==========
    
    // 1. Комиссия → fee_wallet
    transfer_tokens_direct(
        usdt_token_account, fee_wallet_usdt_ata, stablecoin_mint,
        fee_amount, user_wallet,
    )?;
    msg!("  ✅ Комиссия {} USDC переведена на fee_wallet", fee_amount / usdt_multiplier);

    // 2. Чистая сумма → treasury
    transfer_tokens_direct(
        usdt_token_account, treasury_usdt_ata, stablecoin_mint,
        purchase_amount, user_wallet,
    )?;
    msg!("  ✅ Сумма покупки {} USDC переведена в treasury", purchase_amount / usdt_multiplier);

    // ========== ПЕРЕВОД WATER ПОЛЬЗОВАТЕЛЮ (PDA подписывает) ==========
    msg!("  ♒~~~~~~💧: Перевод {} токенов пользователю на ATA", amount_of_tokens / token_multiplier);
    msg!("     ATA адрес: {}", user_project_token_account.key);

    let (_token_fund_pda_calc, bump) = derive_token_fund_pda(program_id, &root_pda)?;

    if _token_fund_pda_calc != *token_fund_pda.key {
        msg!("❌ НЕСООТВЕТСТВИЕ PDA!");
        return Err(WaterError::InvalidPDA.into());
    }

    let transfer_ix = token_instruction::transfer_checked(
        token_program.key,
        token_fund_ata.key,
        token_mint_account.key,
        user_project_token_account.key,
        token_fund_pda.key,
        &[],
        token_lamports,
        mint_decimals,
    ).map_err(|_| WaterError::InvalidTransfer)?;

    invoke_signed(
        &transfer_ix,
        &[
            token_fund_ata.clone(),
            token_mint_account.clone(),
            user_project_token_account.clone(),
            token_fund_pda.clone(),
            token_program.clone(),
        ],
        &[&[TOKEN_FUND_SEED, root_pda.as_ref(), &[0u8], &[bump]]],
    )?;
        
    msg!("  ✅ {} токенов переведены пользователю", amount_of_tokens / token_multiplier);

    // ========== ОБНОВЛЕНИЕ ПОЛЬЗОВАТЕЛЯ ==========
    let mut user = User::get_user(user_account, program_id)?;
    user.buy_token_balance = user.buy_token_balance
        .checked_add(amount_of_tokens).ok_or(WaterError::MathOverflow)?;
    
    user.serialize(&mut *user_account.data.borrow_mut())?;

    // ========== ПЕРЕХОД НА СЛЕДУЮЩУЮ СТАДИЮ ==========
    if sale_config.remaining_tokens == 0 {
        if sale_config.current_stage < (MAX_SALE_STAGES - 1) as u8 {
            let next_stage = sale_config.current_stage.checked_add(1).ok_or(WaterError::MathOverflow)?;
            
            sale_config.current_stage = next_stage;
            sale_config.remaining_tokens = sale_config.stages[next_stage as usize].tokens_available;
            
            sale_config.serialize(&mut *sale_config_account.data.borrow_mut())
                .map_err(|_| WaterError::SerializationError)?;
            
            msg!("➡️ Переход на стадию {}", sale_config.current_stage);
        } else {
            sale_config.is_active = false;
            sale_config.serialize(&mut *sale_config_account.data.borrow_mut())
                .map_err(|_| WaterError::SerializationError)?;
            
            msg!("🏁 Все стадии продаж завершены");
        }
    }

    msg!("✅ УСПЕШНАЯ ПОКУПКА: {} токенов за {} USDC (комиссия: {} USDC, итого: {} USDC)",
        amount_of_tokens / token_multiplier,
        purchase_amount / usdt_multiplier,
        fee_amount / usdt_multiplier,
        total_amount / usdt_multiplier
    );

    Ok(())
}

// ==================== ВСПОМОГАТЕЛЬНАЯ ФУНКЦИЯ ====================

/// Прямой перевод токенов (authority — signer)
fn transfer_tokens_direct<'a>(
    source: &AccountInfo<'a>,
    destination: &AccountInfo<'a>,
    mint_account: &AccountInfo<'a>,
    amount: u64,
    authority: &AccountInfo<'a>,
) -> ProgramResult {
    use solana_program::program::invoke;
    
    let token_program_id = mint_account.owner;
    
    let mint_data = mint_account.data.borrow();
    let mint = StateWithExtensions::<Token2022Mint>::unpack(&mint_data)
        .map_err(|_| WaterError::InvalidAccountData)?;
    let decimals = mint.base.decimals;

    let transfer_ix = token_instruction::transfer_checked(
        token_program_id,
        source.key,
        mint_account.key,
        destination.key,
        authority.key,
        &[],
        amount,
        decimals,
    ).map_err(|_| WaterError::InvalidTransfer)?;

    invoke(
        &transfer_ix,
        &[source.clone(), destination.clone(), mint_account.clone(), authority.clone()],
    )?;

    Ok(())
}