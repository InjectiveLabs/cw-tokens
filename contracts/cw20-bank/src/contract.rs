#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    Coin, attr, coins, to_binary, Addr, BankMsg, Binary, Decimal, Deps, DepsMut, Env,
    MessageInfo, QuerierWrapper, Response,  StdError, StdResult, Uint128, WasmMsg,
};

use injective_cosmwasm::{
    create_mint_tokens_msg,create_burn_tokens_msg, InjectiveMsgWrapper, 
};

use cw2::set_contract_version;
use cw20::{Cw20Coin};
use cw20_base::allowances::{
    execute_burn_from, execute_decrease_allowance, execute_increase_allowance, execute_send_from,
    execute_transfer_from,
};
use cw20_base::contract::{
    execute_burn, execute_send, execute_transfer, query_balance,
};
use cw20_base::state::{MinterData,BALANCES};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg};
use crate::state::{TokenInfo, TOKEN_INFO};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:cw20-bank";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");


#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    // check valid token info
    msg.validate()?;
    // create initial accounts
    let total_supply = create_accounts(&mut deps, &msg.initial_balances)?;

    if let Some(limit) = msg.get_cap() {
        if total_supply > limit {
            return Err(StdError::generic_err("Initial supply greater than cap").into());
        }
    }

    let mint = match msg.mint {
        Some(m) => Some(MinterData {
            minter: deps.api.addr_validate(&m.minter)?,
            cap: m.cap,
        }),
        None => None,
    };

    // store token info
    let data = TokenInfo {
        name: msg.name,
        symbol: msg.symbol,
        decimals: msg.decimals,
        total_supply,
        mint,
        bank_denom: msg.bank_denom,
    };
    TOKEN_INFO.save(deps.storage, &data)?;

    // if let Some(marketing) = msg.marketing {
    //     let logo = if let Some(logo) = marketing.logo {
    //         verify_logo(&logo)?;
    //         LOGO.save(deps.storage, &logo)?;

    //         match logo {
    //             Logo::Url(url) => Some(LogoInfo::Url(url)),
    //             Logo::Embedded(_) => Some(LogoInfo::Embedded),
    //         }
    //     } else {
    //         None
    //     };

    //     let data = MarketingInfoResponse {
    //         project: marketing.project,
    //         description: marketing.description,
    //         marketing: marketing
    //             .marketing
    //             .map(|addr| deps.api.addr_validate(&addr))
    //             .transpose()?,
    //         logo,
    //     };
    //     MARKETING_INFO.save(deps.storage, &data)?;
    // }

    Ok(Response::default())
}


pub fn create_accounts(
    deps: &mut DepsMut,
    accounts: &[Cw20Coin],
) -> Result<Uint128, ContractError> {
    validate_accounts(accounts)?;

    let mut total_supply = Uint128::zero();
    for row in accounts {
        let address = deps.api.addr_validate(&row.address)?;
        BALANCES.save(deps.storage, &address, &row.amount)?;
        total_supply += row.amount;
    }

    Ok(total_supply)
}

pub fn validate_accounts(accounts: &[Cw20Coin]) -> Result<(), ContractError> {
    let mut addresses = accounts.iter().map(|c| &c.address).collect::<Vec<_>>();
    addresses.sort();
    addresses.dedup();

    if addresses.len() != accounts.len() {
        Err(ContractError::DuplicateInitialBalanceAddresses {})
    } else {
        Ok(())
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    match msg {
        ExecuteMsg::Cw20ToBank { amount } => execute_cw20_to_bank(deps, env, info, amount),
        ExecuteMsg::BankToCw20 {} => execute_bank_to_cw20(deps, env, info),

        // // these all come from cw20-base to implement the cw20 standard
        ExecuteMsg::Transfer { recipient, amount } => {
            Ok(execute_transfer(deps, env, info, recipient, amount)?)
        }
        // ExecuteMsg::Burn { amount } => Ok(execute_burn(deps, env, info, amount)?),
        // ExecuteMsg::Send {
        //     contract,
        //     amount,
        //     msg,
        // } => Ok(execute_send(deps, env, info, contract, amount, msg)?),
        // ExecuteMsg::IncreaseAllowance {
        //     spender,
        //     amount,
        //     expires,
        // } => Ok(execute_increase_allowance(
        //     deps, env, info, spender, amount, expires,
        // )?),
        // ExecuteMsg::DecreaseAllowance {
        //     spender,
        //     amount,
        //     expires,
        // } => Ok(execute_decrease_allowance(
        //     deps, env, info, spender, amount, expires,
        // )?),
        // ExecuteMsg::TransferFrom {
        //     owner,
        //     recipient,
        //     amount,
        // } => Ok(execute_transfer_from(
        //     deps, env, info, owner, recipient, amount,
        // )?),
        // ExecuteMsg::BurnFrom { owner, amount } => {
        //     Ok(execute_burn_from(deps, env, info, owner, amount)?)
        // }
        // ExecuteMsg::SendFrom {
        //     owner,
        //     contract,
        //     amount,
        //     msg,
        // } => Ok(execute_send_from(
        //     deps, env, info, owner, contract, amount, msg,
        // )?),
    }
}


pub fn execute_transfer_wrapper(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let rcpt_addr = deps.api.addr_validate(&recipient)?;

    BALANCES.update(
        deps.storage,
        &info.sender,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let res = Response::new()
        .add_attribute("action", "transfer")
        .add_attribute("from", info.sender)
        .add_attribute("to", recipient)
        .add_attribute("amount", amount);
    Ok(res)
}


pub fn execute_cw20_to_bank(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }
    // Verify Cw20 balance of the sender
    let balance = query_balance(deps.as_ref(), info.sender.to_string())
        .unwrap()
        .balance;
    if amount > balance {
        return Err(ContractError::InsufficientBalance {});
    }

    // Burn cw20 tokens of the user
    // lower balance
    BALANCES.update(
        deps.storage,
        &info.sender,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;
    // reduce total_supply
    TOKEN_INFO.update(deps.storage, |mut info| -> StdResult<_> {
        info.total_supply = info.total_supply.checked_sub(amount)?;
        Ok(info)
    })?;

    // transfer the bank denom to user address from CW20 contract address.
    // Create and submit BankTransfer sub message. Here sender is the contract address.
    let config = TOKEN_INFO.load(deps.storage)?;

    let sender = env.contract.address.to_string();
    let mint_to_address =  info.sender.to_string();

    let mint_tokens_msg = create_mint_tokens_msg(
        sender.clone(),        
        Coin::new(amount.u128(), config.bank_denom.unwrap()),
        mint_to_address,
    );    

    let res = Response::new().add_message(mint_tokens_msg);
    Ok(res)
}

pub fn execute_bank_to_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    let mut config = TOKEN_INFO.load(deps.storage)?;

    // Make sure the user has transferred same bank denom to the contract address.
    let coin = &info.funds[0];
    match &config.bank_denom {
        None => return Err(ContractError::BankDenomNotSet {}), // Bank denom is not set
        Some(bank_denom) if (*bank_denom == coin.denom) => {} // The transfered token match with Bank denom
        Some(_) => return Err(ContractError::InvalidBankDenom {}), // The transfered token doesn't match with Bank denom
    }

    let amount = coin.amount;
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    // Mint CW20 tokens for user address.
    config.total_supply += amount;
    if let Some(limit) = config.get_cap() {
        if config.total_supply > limit {
            return Err(ContractError::CannotExceedCap {});
        }
    }
    TOKEN_INFO.save(deps.storage, &config)?;

    // add cw20 token amount to sender balance
    let rcpt_addr = deps.api.addr_validate(info.sender.as_str())?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    // let res = Response::new()
    //     .add_attribute("action", "bank_to_cw20")
    //     .add_attribute("from", info.sender)
    //     .add_attribute("amount", amount);
    // Ok(res)

    let sender = env.contract.address.to_string();

    let burn_tokens_msg = create_burn_tokens_msg(
        sender.clone(),                
        Coin::new(amount.u128(), config.bank_denom.unwrap()),
        sender,
    );

    Ok(Response::new()
    .add_message(burn_tokens_msg)
    .add_attributes(vec![
        attr("denom", "denom"),
        attr("burn_from_address", "burn_from_address"),        
    ]))
}