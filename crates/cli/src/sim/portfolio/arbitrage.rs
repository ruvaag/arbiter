use std::error::Error;

use bindings::rmm01_portfolio;
use ethers::{prelude::U256, types::I256};
use eyre::Result;
use revm::primitives::{ruint::Uint, B160};
use simulate::{
    agent::{simple_arbitrageur::SimpleArbitrageur, Agent, AgentType, SimulationEventFilter},
    contract::{IsDeployed, SimulationContract},
    manager::{SimulationManager, self}, utils::float_to_wad,
};

pub(crate) fn create_arbitrageur<S: Into<String>>(
    manager: &mut SimulationManager,
    liquid_exchange: &SimulationContract<IsDeployed>,
    name: S,
) {
    let event_filters = vec![SimulationEventFilter::new(liquid_exchange, "PriceChange")];
    let arbitrageur = SimpleArbitrageur::new(name, event_filters);
    manager
        .activate_agent(
            AgentType::SimpleArbitrageur(arbitrageur),
            B160::from_low_u64_be(2),
        )
        .unwrap();
}

pub(crate) fn compute_arb_size(
    current_price: U256,
    target_price: U256,
    manager: &mut SimulationManager,
    pool_params: &rmm01_portfolio::CreatePoolCall,
    pool_id: u64,
    portfolio: &SimulationContract<IsDeployed>,
) -> Result<(), Box<dyn Error>> {
    let manager = manager;
    let admin = manager.agents.get("admin").unwrap();
    let arbiter_math = manager.autodeployed_contracts.get("arbiter_math").unwrap();

    let strike = U256::from(pool_params.strike_price);
    let iv = U256::from(pool_params.volatility);
    let duration = U256::from(pool_params.duration);

    // compute time term
    let execution_result = admin.call_contract(
        &mut manager.environment,
        &arbiter_math,
        arbiter_math.encode_function(
            "sqrt",
            (duration)
        )?,
        Uint::ZERO,
    );
    let unpacked_result = manager.unpack_execution(execution_result)?;
    let time_term: U256 = arbiter_math.decode_output("sqrt", unpacked_result)?;
    // compute sigma*sqrt(tau)
    let execution_result = admin.call_contract(
        &mut manager.environment,
        &arbiter_math,
        arbiter_math.encode_function(
            "mulWadUp",
            (iv, time_term)
        )?,
        Uint::ZERO,
    );
    let unpacked_result = manager.unpack_execution(execution_result)?;
    let sigma_sqrt_tau: U256 = arbiter_math.decode_output("mulWadUp", unpacked_result)?;
    // compute the ratio
    let execution_result = admin.call_contract(
        &mut manager.environment,
        &arbiter_math,
        arbiter_math.encode_function(
            "divWadUp",
            (target_price, strike)
        )?,
        Uint::ZERO,
    );
    let unpacked_result = manager.unpack_execution(execution_result)?;
    let ratio: U256 = arbiter_math.decode_output("divWadUp", unpacked_result)?;
    // compute logarithm
    let execution_result = admin.call_contract(
        &mut manager.environment,
        &arbiter_math,
        arbiter_math.encode_function(
            "log",
            ratio // convert to I256
        )?,
        Uint::ZERO,
    );
    let unpacked_result = manager.unpack_execution(execution_result)?;
    let log: I256 = arbiter_math.decode_output("log", unpacked_result)?;
    // Scale logarithm
    let execution_result = admin.call_contract(
        &mut manager.environment,
        &arbiter_math,
        arbiter_math.encode_function(
            "mulWadUp",
            (U256::from(log), sigma_sqrt_tau)
        )?,
        Uint::ZERO,
    );
    let unpacked_result = manager.unpack_execution(execution_result)?;
    let scaled_log: U256 = arbiter_math.decode_output("mulWadUp", unpacked_result)?;
    // compute the additional term 
    let execution_result = admin.call_contract(
        &mut manager.environment,
        &arbiter_math,
        arbiter_math.encode_function(
            "mulWadDown",
            (U256::from(500_000_000_000_000_000_u128), sigma_sqrt_tau)
        )?,
        Uint::ZERO,
    );
    let unpacked_result = manager.unpack_execution(execution_result)?;
    let additional_term: U256 = arbiter_math.decode_output("mulWadDown", unpacked_result)?;
    // CDF input
    let cdf_input = scaled_log + additional_term;
    // compute the CDF
    let execution_result = admin.call_contract(
        &mut manager.environment,
        &arbiter_math,
        arbiter_math.encode_function(
            "cdf", 
            cdf_input
        )?,
        Uint::ZERO,
    );
    let unpacked_result = manager.unpack_execution(execution_result)?;
    let cdf_output: I256 = arbiter_math.decode_output("cdf", unpacked_result)?;
    // compute the arb size
    let x_reserves = admin.call_contract(
        &mut manager.environment,
         &portfolio, 
         portfolio.encode_function("getVirtualReservesDec", pool_id)?, 
         Uint::ZERO,
        );
    let unpacked_result = manager.unpack_execution(execution_result)?;
    let x_reserves: (u128, u128) = portfolio.decode_output("getVirtualReservesDec", unpacked_result)?;

    let a = cdf_output;
    Ok(())
}

pub(crate) fn swap(
    manager: &mut SimulationManager,
    portfolio: &SimulationContract<IsDeployed>,
    pool_id: u64,
) -> Result<(), Box<dyn Error>> {
    let arbitrageur = manager.agents.get("arbitrageur").unwrap();
    // --------------------------------------------------------------------------------------------
    // PORTFOLIO POOL SWAP
    // --------------------------------------------------------------------------------------------
    // Get the correct amount of ARBY to get from a certain amount of ARBX using `getAmountOut`
    let input_amount = 1_000_000; // This causes InvalidInvariant revert.
    let get_amount_out_args = rmm01_portfolio::GetAmountOutCall {
        pool_id,                               // pool_id: u64,
        sell_asset: false, /* sell_asset: bool, // Setting this to true means we are selling ARBX for ARBY */
        amount_in: U256::from(input_amount), // amount_in: ::ethers::core::types::U256,
        liquidity_delta: I256::from(0), // liquidity_delta: ::ethers::core::types::I256,
        swapper: arbitrageur.address().into(), // swapper: ::ethers::core::types::Address,
    };
    let get_amount_out_result = arbitrageur.call_contract(
        &mut manager.environment,
        portfolio,
        portfolio.encode_function("getAmountOut", get_amount_out_args)?,
        Uint::from(0),
    );
    assert!(get_amount_out_result.is_success());
    let unpacked_get_amount_out = manager.unpack_execution(get_amount_out_result)?;
    let decoded_amount_out: u128 =
        portfolio.decode_output("getAmountOut", unpacked_get_amount_out)?;
    println!(
        "Inputting {} ARBX yields {} ARBY out.",
        input_amount, decoded_amount_out,
    );

    // Construct the swap using the above amount
    let amount_out = decoded_amount_out;
    let order = rmm01_portfolio::Order {
        input: input_amount as u128,
        output: amount_out,
        use_max: false,
        pool_id,
        sell_asset: false,
    };
    let swap_args = (order,);
    let swap_result = arbitrageur.call_contract(
        &mut manager.environment,
        portfolio,
        portfolio.encode_function("swap", swap_args)?,
        Uint::from(0),
    );
    match manager.unpack_execution(swap_result) {
        Ok(unpacked) => {
            let swap_result: (u64, U256, U256) = portfolio.decode_output("swap", unpacked)?;
            println!("Swap result is {:#?}", swap_result);
        }
        Err(e) => {
            // This `InvalidInvariant` can pop up in multiple ways. Best to check for this.
            let value = e.output.unwrap();
            let decoded_result = portfolio.decode_error("InvalidInvariant".to_string(), value);
            println!("The result of `InvalidInvariant` is: {:#?}", decoded_result)
        }
    };
    Ok(())
}
