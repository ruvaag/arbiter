use std::{env, fs, io::Empty, str::FromStr, sync::Arc};

use crate::tests::weth;

use super::*;

use config::{Config, ConfigError};
use ethers::{
    providers::{Http, Middleware, Provider},
    types::{Address, Block, BlockId, BlockNumber, U256},
    utils::hex,
};
use revm::{
    db::{ethersdb::EthersDB, CacheDB, EmptyDB},
    primitives::{hex_literal::hex, B160},
    Database, InMemoryDB,
};
use serde::{Deserialize, Serialize};

use arbiter_core::{
    environment::{self, Environment},
    middleware::RevmMiddleware,
};
use ethers::utils::keccak256;
use tests::weth::WETH;

// NOTES:
// We should probably read in some kind of hashmap of addresses to give contract names
// that we can then get addresses from so we can set up the middleware stuff correctly

// Need some simple contract that is deployed that we can do some tests with since we will want bindings.

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ForkConfig {
    output_path: String,
    filename: String,
    block_number: u64,
    addresses: Vec<Address>,
}

impl ForkConfig {
    pub fn new(fork_config: &String) -> Result<Self, ConfigError> {
        let s = Config::builder()
            .add_source(config::File::with_name(fork_config))
            .build()?;
        s.try_deserialize()
    }
}
#[test]
// pub fn fork(fork_config: &String) -> Result<(), ConfigurationError> {
pub fn fork_write_out() -> Result<(), ConfigurationError> {
    let fork_config = &"fork_config.toml".to_owned();
    let fork_config = ForkConfig::new(fork_config).unwrap();
    let client = Arc::new(
        Provider::<Http>::try_from("https://mainnet.infura.io/v3/c60b0bb42f8a4c6481ecd229eddaca27")
            .expect("could not instantiate HTTP Provider"),
    );
    let mut test_db = CacheDB::new(EmptyDB::default());
    let block_id = BlockId::Number(BlockNumber::Number(fork_config.block_number.into()));
    let mut db = EthersDB::new(client, Some(block_id)).unwrap();
    for address in fork_config.addresses {
        let thing = db.basic(address.into()).unwrap().unwrap();
        test_db.insert_contract(&mut thing.clone());
        println!("The thing itself has:: {}: {:?}", address, thing);
        println!("The DB itself is: {:?}", test_db);
    }
    println!("Outputting to: {:?}", fork_config.output_path);
    let current_dir = env::current_dir()?;
    let output_dir = current_dir.join(fork_config.output_path);
    fs::create_dir_all(&output_dir)?;
    let file_path = output_dir.join(fork_config.filename);
    serde_json::to_writer_pretty(
        std::fs::File::create(file_path).unwrap(),
        &test_db.contracts,
    );
    Ok(())
}

#[test]
// pub fn fork(fork_config: &String) -> Result<(), ConfigurationError> {
pub fn fork_weth() -> Result<(), ConfigurationError> {
    let fork_config = &"fork_config.toml".to_owned();
    let fork_config = ForkConfig::new(fork_config).unwrap();
    let client = Arc::new(
        Provider::<Http>::try_from("https://eth.llamarpc.com")
            .expect("could not instantiate HTTP Provider"),
    );
    let mut test_db = CacheDB::new(EmptyDB::default());
    let block_id = BlockId::Number(BlockNumber::Number(fork_config.block_number.into()));
    let mut db = EthersDB::new(client, Some(block_id)).unwrap();
    let address = fork_config.addresses[0];
    let account_info = db.basic(address.into()).unwrap().unwrap();
    test_db.insert_account_info(address.into(), account_info.clone());
    for index in 0..7 {
        if let Ok(storage) = db.storage(address.into(), revm::primitives::U256::from(index)) {
            println!("Index: {:?}", index);
            println!("Storage: {:?}", storage);
            test_db.insert_account_storage(
                address.into(),
                revm::primitives::U256::from(index),
                storage,
            );
        } else {
            panic!("something bad happened");
        }
    }
    let test_account_address =
        Address::from_str("0x6B44ba0a126a2A1a8aa6cD1AdeeD002e141Bcd44").unwrap();

    let test_index = revm::primitives::U256::from(3).to_be_bytes_vec();
    println!("test_index: {:?}", test_index);
    let test_account_address_bytes: Vec<u8> = test_account_address.to_fixed_bytes().to_vec();
    let mut padded: Vec<u8> = vec![0; 12];
    padded.extend(test_account_address_bytes);
    println!("paded_test_account_address_bytes: {:?}", padded);
    let test_bytes: Vec<u8> = padded.into_iter().chain(test_index).collect();
    println!("test_bytes: {:?}", test_bytes);
    println!("test_bytes.len(): {:?}", test_bytes.len());
    let test_slot = keccak256(test_bytes);
    println!("hex of test slot: {:?}", hex::encode(test_slot));
    println!("test_slot: {:?}", test_slot);
    if let Ok(storage) = db.storage(
        address.into(),
        revm::primitives::U256::from_be_bytes(test_slot),
    ) {
        println!("Storage: {:?}", storage);
        test_db.insert_account_storage(
            address.into(),
            revm::primitives::U256::from_be_bytes(test_slot.into()),
            storage,
        );
    } else {
        panic!("something bad happened");
    }

    let mut environment = environment::builder::EnvironmentBuilder::new()
        .db(test_db)
        .build();
    environment.run();
    let client = Arc::new(RevmMiddleware::new(&environment, Some("name")).unwrap());

    // println!("the db is: {:?}", environment.db);

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let weth = WETH::new(address, client.clone());
        let decimals = weth.decimals().call().await.unwrap();
        println!("decimals: {:?}", decimals);
        let balance_call = weth.balance_of(test_account_address);
        println!("balance_call: {:?}", balance_call);
        let balance = balance_call.call().await.unwrap();
        println!("weth balance: {:?}", balance);
    });

    // println!("accounts: {:?}", client.get_accounts().await);

    Ok(())
}
