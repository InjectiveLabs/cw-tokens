#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, coins, to_binary, Addr, BankMsg, Binary, Decimal, Deps, DepsMut, DistributionMsg, Env,
    MessageInfo, QuerierWrapper, Response, StakingMsg, StdError, StdResult, Uint128, WasmMsg,
};

use cw2::set_contract_version;
use cw20::{
    BalanceResponse, Cw20Coin,  Cw20ReceiveMsg, DownloadLogoResponse, EmbeddedLogo, Logo, LogoInfo,
    MarketingInfoResponse, MinterResponse, TokenInfoResponse,
};
use cw20_base::allowances::{
    execute_burn_from, execute_decrease_allowance, execute_increase_allowance, execute_send_from,
    execute_transfer_from, query_allowance,
};
use cw20_base::contract::{
    execute_burn, execute_mint, execute_send, execute_transfer, query_balance, query_token_info,
};
use cw20_base::state::{MinterData,BALANCES};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, InvestmentResponse, QueryMsg};
use crate::state::{TokenInfo, TOKEN_INFO, InvestmentInfo, Supply, CLAIMS, INVESTMENT, TOTAL_SUPPLY};

const FALLBACK_RATIO: Decimal = Decimal::one();

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
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Cw20ToBank { amount } => execute_cw20_to_bank(deps, env, info, amount),
        ExecuteMsg::BankToCw20 {} => execute_bank_to_cw20(deps, env, info),

        ExecuteMsg::Bond {} => bond(deps, env, info),
        ExecuteMsg::Unbond { amount } => unbond(deps, env, info, amount),
        ExecuteMsg::Claim {} => claim(deps, env, info),
        ExecuteMsg::Reinvest {} => reinvest(deps, env, info),
        ExecuteMsg::_BondAllTokens {} => _bond_all_tokens(deps, env, info),

        // these all come from cw20-base to implement the cw20 standard
        ExecuteMsg::Transfer { recipient, amount } => {
            Ok(execute_transfer(deps, env, info, recipient, amount)?)
        }
        ExecuteMsg::Burn { amount } => Ok(execute_burn(deps, env, info, amount)?),
        ExecuteMsg::Send {
            contract,
            amount,
            msg,
        } => Ok(execute_send(deps, env, info, contract, amount, msg)?),
        ExecuteMsg::IncreaseAllowance {
            spender,
            amount,
            expires,
        } => Ok(execute_increase_allowance(
            deps, env, info, spender, amount, expires,
        )?),
        ExecuteMsg::DecreaseAllowance {
            spender,
            amount,
            expires,
        } => Ok(execute_decrease_allowance(
            deps, env, info, spender, amount, expires,
        )?),
        ExecuteMsg::TransferFrom {
            owner,
            recipient,
            amount,
        } => Ok(execute_transfer_from(
            deps, env, info, owner, recipient, amount,
        )?),
        ExecuteMsg::BurnFrom { owner, amount } => {
            Ok(execute_burn_from(deps, env, info, owner, amount)?)
        }
        ExecuteMsg::SendFrom {
            owner,
            contract,
            amount,
            msg,
        } => Ok(execute_send_from(
            deps, env, info, owner, contract, amount, msg,
        )?),
    }
}

pub fn execute_cw20_to_bank(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
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

    let bank_transfer = BankMsg::Send {
        to_address: info.sender.to_string(),
        amount: coins(amount.u128(), config.bank_denom.unwrap()),
    };

    let res = Response::new().add_message(bank_transfer);
    Ok(res)
}

pub fn execute_bank_to_cw20(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
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

    let res = Response::new()
        .add_attribute("action", "bank_to_cw20")
        .add_attribute("from", info.sender)
        .add_attribute("amount", amount);
    Ok(res)
}
// get_bonded returns the total amount of delegations from contract
// it ensures they are all the same denom
fn get_bonded(querier: &QuerierWrapper, contract: &Addr) -> Result<Uint128, ContractError> {
    let bonds = querier.query_all_delegations(contract)?;
    if bonds.is_empty() {
        return Ok(Uint128::zero());
    }
    let denom = bonds[0].amount.denom.as_str();
    bonds.iter().fold(Ok(Uint128::zero()), |racc, d| {
        let acc = racc?;
        if d.amount.denom.as_str() != denom {
            Err(ContractError::DifferentBondDenom {
                denom1: denom.into(),
                denom2: d.amount.denom.to_string(),
            })
        } else {
            Ok(acc + d.amount.amount)
        }
    })
}

fn assert_bonds(supply: &Supply, bonded: Uint128) -> Result<(), ContractError> {
    if supply.bonded != bonded {
        Err(ContractError::BondedMismatch {
            stored: supply.bonded,
            queried: bonded,
        })
    } else {
        Ok(())
    }
}

pub fn bond(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    // ensure we have the proper denom
    let invest = INVESTMENT.load(deps.storage)?;
    // payment finds the proper coin (or throws an error)
    let payment = info
        .funds
        .iter()
        .find(|x| x.denom == invest.bond_denom)
        .ok_or_else(|| ContractError::EmptyBalance {
            denom: invest.bond_denom.clone(),
        })?;

    // bonded is the total number of tokens we have delegated from this address
    let bonded = get_bonded(&deps.querier, &env.contract.address)?;

    // calculate to_mint and update total supply
    let mut supply = TOTAL_SUPPLY.load(deps.storage)?;
    // TODO: this is just a safety assertion - do we keep it, or remove caching?
    // in the end supply is just there to cache the (expected) results of get_bonded() so we don't
    // have expensive queries everywhere
    assert_bonds(&supply, bonded)?;
    let to_mint = if supply.issued.is_zero() || bonded.is_zero() {
        FALLBACK_RATIO * payment.amount
    } else {
        payment.amount.multiply_ratio(supply.issued, bonded)
    };
    supply.bonded = bonded + payment.amount;
    supply.issued += to_mint;
    TOTAL_SUPPLY.save(deps.storage, &supply)?;

    // call into cw20-base to mint the token, call as self as no one else is allowed
    let sub_info = MessageInfo {
        sender: env.contract.address.clone(),
        funds: vec![],
    };
    execute_mint(deps, env, sub_info, info.sender.to_string(), to_mint)?;

    // bond them to the validator
    let res = Response::new()
        .add_message(StakingMsg::Delegate {
            validator: invest.validator,
            amount: payment.clone(),
        })
        .add_attribute("action", "bond")
        .add_attribute("from", info.sender)
        .add_attribute("bonded", payment.amount)
        .add_attribute("minted", to_mint);
    Ok(res)
}

pub fn unbond(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let invest = INVESTMENT.load(deps.storage)?;
    // ensure it is big enough to care
    if amount < invest.min_withdrawal {
        return Err(ContractError::UnbondTooSmall {
            min_bonded: invest.min_withdrawal,
            denom: invest.bond_denom,
        });
    }
    // calculate tax and remainer to unbond
    let tax = amount * invest.exit_tax;

    // burn from the original caller
    execute_burn(deps.branch(), env.clone(), info.clone(), amount)?;
    if tax > Uint128::zero() {
        let sub_info = MessageInfo {
            sender: env.contract.address.clone(),
            funds: vec![],
        };
        // call into cw20-base to mint tokens to owner, call as self as no one else is allowed
        execute_mint(
            deps.branch(),
            env.clone(),
            sub_info,
            invest.owner.to_string(),
            tax,
        )?;
    }

    // re-calculate bonded to ensure we have real values
    // bonded is the total number of tokens we have delegated from this address
    let bonded = get_bonded(&deps.querier, &env.contract.address)?;

    // calculate how many native tokens this is worth and update supply
    let remainder = amount.checked_sub(tax).map_err(StdError::overflow)?;
    let mut supply = TOTAL_SUPPLY.load(deps.storage)?;
    // TODO: this is just a safety assertion - do we keep it, or remove caching?
    // in the end supply is just there to cache the (expected) results of get_bonded() so we don't
    // have expensive queries everywhere
    assert_bonds(&supply, bonded)?;
    let unbond = remainder.multiply_ratio(bonded, supply.issued);
    supply.bonded = bonded.checked_sub(unbond).map_err(StdError::overflow)?;
    supply.issued = supply
        .issued
        .checked_sub(remainder)
        .map_err(StdError::overflow)?;
    supply.claims += unbond;
    TOTAL_SUPPLY.save(deps.storage, &supply)?;

    CLAIMS.create_claim(
        deps.storage,
        &info.sender,
        unbond,
        invest.unbonding_period.after(&env.block),
    )?;

    // unbond them
    let res = Response::new()
        .add_message(StakingMsg::Undelegate {
            validator: invest.validator,
            amount: coin(unbond.u128(), &invest.bond_denom),
        })
        .add_attribute("action", "unbond")
        .add_attribute("to", info.sender)
        .add_attribute("unbonded", unbond)
        .add_attribute("burnt", amount);
    Ok(res)
}

pub fn claim(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    // find how many tokens the contract has
    let invest = INVESTMENT.load(deps.storage)?;
    let mut balance = deps
        .querier
        .query_balance(&env.contract.address, &invest.bond_denom)?;
    if balance.amount < invest.min_withdrawal {
        return Err(ContractError::BalanceTooSmall {});
    }

    // check how much to send - min(balance, claims[sender]), and reduce the claim
    // Ensure we have enough balance to cover this and only send some claims if that is all we can cover
    let to_send =
        CLAIMS.claim_tokens(deps.storage, &info.sender, &env.block, Some(balance.amount))?;
    if to_send == Uint128::zero() {
        return Err(ContractError::NothingToClaim {});
    }

    // update total supply (lower claim)
    TOTAL_SUPPLY.update(deps.storage, |mut supply| -> StdResult<_> {
        supply.claims = supply.claims.checked_sub(to_send)?;
        Ok(supply)
    })?;

    // transfer tokens to the sender
    balance.amount = to_send;
    let res = Response::new()
        .add_message(BankMsg::Send {
            to_address: info.sender.to_string(),
            amount: vec![balance],
        })
        .add_attribute("action", "claim")
        .add_attribute("from", info.sender)
        .add_attribute("amount", to_send);
    Ok(res)
}

/// reinvest will withdraw all pending rewards,
/// then issue a callback to itself via _bond_all_tokens
/// to reinvest the new earnings (and anything else that accumulated)
pub fn reinvest(deps: DepsMut, env: Env, _info: MessageInfo) -> Result<Response, ContractError> {
    let contract_addr = env.contract.address;
    let invest = INVESTMENT.load(deps.storage)?;
    let msg = to_binary(&ExecuteMsg::_BondAllTokens {})?;

    // and bond them to the validator
    let res = Response::new()
        .add_message(DistributionMsg::WithdrawDelegatorReward {
            validator: invest.validator,
        })
        .add_message(WasmMsg::Execute {
            contract_addr: contract_addr.to_string(),
            msg,
            funds: vec![],
        });
    Ok(res)
}

pub fn _bond_all_tokens(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    // this is just meant as a call-back to ourself
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }

    // find how many tokens we have to bond
    let invest = INVESTMENT.load(deps.storage)?;
    let mut balance = deps
        .querier
        .query_balance(&env.contract.address, &invest.bond_denom)?;

    // we deduct pending claims from our account balance before reinvesting.
    // if there is not enough funds, we just return a no-op
    match TOTAL_SUPPLY.update(deps.storage, |mut supply| -> StdResult<_> {
        balance.amount = balance.amount.checked_sub(supply.claims)?;
        // this just triggers the "no op" case if we don't have min_withdrawal left to reinvest
        balance.amount.checked_sub(invest.min_withdrawal)?;
        supply.bonded += balance.amount;
        Ok(supply)
    }) {
        Ok(_) => {}
        // if it is below the minimum, we do a no-op (do not revert other state from withdrawal)
        Err(StdError::Overflow { .. }) => return Ok(Response::default()),
        Err(e) => return Err(ContractError::Std(e)),
    }

    // and bond them to the validator
    let res = Response::new()
        .add_message(StakingMsg::Delegate {
            validator: invest.validator,
            amount: balance.clone(),
        })
        .add_attribute("action", "reinvest")
        .add_attribute("bonded", balance.amount);
    Ok(res)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        // custom queries
        QueryMsg::Claims { address } => {
            to_binary(&CLAIMS.query_claims(deps, &deps.api.addr_validate(&address)?)?)
        }
        QueryMsg::Investment {} => to_binary(&query_investment(deps)?),
        // inherited from cw20-base
        QueryMsg::TokenInfo {} => to_binary(&query_token_info(deps)?),
        QueryMsg::Balance { address } => to_binary(&query_balance(deps, address)?),
        QueryMsg::Allowance { owner, spender } => {
            to_binary(&query_allowance(deps, owner, spender)?)
        }
    }
}

pub fn query_investment(deps: Deps) -> StdResult<InvestmentResponse> {
    let invest = INVESTMENT.load(deps.storage)?;
    let supply = TOTAL_SUPPLY.load(deps.storage)?;

    let res = InvestmentResponse {
        owner: invest.owner.to_string(),
        exit_tax: invest.exit_tax,
        validator: invest.validator,
        min_withdrawal: invest.min_withdrawal,
        token_supply: supply.issued,
        staked_tokens: coin(supply.bonded.u128(), &invest.bond_denom),
        nominal_value: if supply.issued.is_zero() {
            FALLBACK_RATIO
        } else {
            Decimal::from_ratio(supply.bonded, supply.issued)
        },
    };
    Ok(res)
}
