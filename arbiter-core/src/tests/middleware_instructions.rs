use ethers::types::transaction::eip2718::TypedTransaction;

use super::*;
use crate::middleware::nonce_middleware::NonceManagerMiddleware;

#[tokio::test]
async fn deploy() {
    let (_environment, client) = startup_user_controlled().unwrap();
    let arbiter_token = deploy_arbx(client).await.unwrap();
    assert_eq!(
        arbiter_token.address(),
        Address::from_str("0x067ea9e44c76a2620f10b39a1b51d5124a299192").unwrap()
    );
}

#[tokio::test]
async fn call() {
    let (_environment, client) = startup_user_controlled().unwrap();
    let arbiter_token = deploy_arbx(client).await.unwrap();
    let admin = arbiter_token.admin();
    let output = admin.call().await.unwrap();
    assert_eq!(
        output,
        Address::from_str("0x2efdc9eecfee3a776209fcb8e9a83a6b221d74f5").unwrap()
    );
}

#[tokio::test]
async fn transact() {
    let (_environment, client) = startup_user_controlled().unwrap();
    let arbiter_token = deploy_arbx(client).await.unwrap();
    let mint = arbiter_token.mint(
        Address::from_str(TEST_MINT_TO).unwrap(),
        ethers::types::U256::from(TEST_MINT_AMOUNT),
    );
    let receipt = mint.send().await.unwrap().await.unwrap().unwrap();
    assert_eq!(receipt.logs[0].address, arbiter_token.address());
    let topics = vec![
        ethers::core::types::H256::from_str(
            "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
        )
        .unwrap(),
        ethers::core::types::H256::from_str(
            "0x0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap(),
        ethers::core::types::H256::from_str(
            "0x000000000000000000000000f7e93cc543d97af6632c9b8864417379dba4bf15",
        )
        .unwrap(),
    ];
    assert_eq!(receipt.logs[0].topics, topics);
    let bytes =
        hex::decode("0000000000000000000000000000000000000000000000000000000000000045").unwrap();
    assert_eq!(
        receipt.logs[0].data,
        ethers::core::types::Bytes::from(bytes)
    );
    println!("logs are: {:#?}", receipt.logs);
}

#[tokio::test]
async fn filter_id() {
    let (_environment, client) = startup_user_controlled().unwrap();
    let arbiter_token = deploy_arbx(client.clone()).await.unwrap();
    let filter_watcher_1 = client.watch(&Filter::default()).await.unwrap();
    let filter_watcher_2 = client
        .watch(&Filter::new().address(arbiter_token.address()))
        .await
        .unwrap();
    assert_ne!(filter_watcher_1.id, filter_watcher_2.id);
}

#[tokio::test]
async fn filter_watcher() {
    let (_environment, client) = startup_user_controlled().unwrap();
    let arbiter_token = deploy_arbx(client.clone()).await.unwrap();
    let mut filter_watcher = client.watch(&Filter::default()).await.unwrap();
    let approval = arbiter_token.approve(
        client.default_sender().unwrap(),
        ethers::types::U256::from(TEST_APPROVAL_AMOUNT),
    );
    approval.send().await.unwrap().await.unwrap();
    let event = filter_watcher.next().await.unwrap();
    assert_eq!(event.address, arbiter_token.address());
    // Check that the only populated topic from the approval_filter is correct
    let filter_topic = match arbiter_token.approval_filter().filter.topics[0]
        .clone()
        .unwrap()
    {
        ValueOrArray::Value(filter) => filter.unwrap(),
        _ => panic!("Expected ValueOrArray::Value"),
    };
    assert_eq!(event.topics[0], filter_topic);
    assert_eq!(
        event.data,
        ethers::types::Bytes::from_str(
            "0x00000000000000000000000000000000000000000000000000000000000001a4"
        )
        .unwrap()
    );
    let approval_filter_output = ApprovalFilter::decode_log(&event.into()).unwrap();
    println!("Decoded Log: {:#?}", approval_filter_output);
    assert_eq!(
        approval_filter_output.owner,
        client.default_sender().unwrap()
    );
    assert_eq!(
        approval_filter_output.spender,
        client.default_sender().unwrap()
    );
    assert_eq!(
        approval_filter_output.amount,
        ethers::types::U256::from(TEST_APPROVAL_AMOUNT)
    );
}

#[tokio::test]
async fn filter_address() {
    let (_environment, client) = startup_user_controlled().unwrap();
    let arbiter_token = deploy_arbx(client.clone()).await.unwrap();

    let mut default_watcher = client.watch(&Filter::default()).await.unwrap();
    let mut address_watcher = client
        .watch(&Filter::new().address(arbiter_token.address()))
        .await
        .unwrap();

    // Check that both watchers get this event
    let approval = arbiter_token.approve(
        client.default_sender().unwrap(),
        ethers::types::U256::from(TEST_APPROVAL_AMOUNT),
    );
    approval.send().await.unwrap().await.unwrap();
    let default_watcher_event = default_watcher.next().await.unwrap();
    let address_watcher_event = address_watcher.next().await.unwrap();
    assert!(!default_watcher_event.data.is_empty());
    assert!(!address_watcher_event.data.is_empty());
    assert_eq!(default_watcher_event, address_watcher_event);

    // Create a new token contract to check that the address watcher only gets
    // events from the correct contract.
    // Check that only the default watcher gets
    // this event
    let arbiter_token2 = ArbiterToken::deploy(
        client.clone(),
        (
            format!("new_{}", TEST_ARG_NAME),
            format!("new_{}", TEST_ARG_SYMBOL),
            TEST_ARG_DECIMALS,
        ),
    )
    .unwrap()
    .send()
    .await
    .unwrap();

    // Sanity check that tokens have different addresses
    assert_ne!(arbiter_token.address(), arbiter_token2.address());

    // Send the tx that generates the event
    let approval = arbiter_token2.approve(
        client.default_sender().unwrap(),
        ethers::types::U256::from(TEST_APPROVAL_AMOUNT),
    );
    approval.send().await.unwrap().await.unwrap();

    // get the next event with the default_watcher
    let event_two = default_watcher.next().await.unwrap();
    assert!(!event_two.data.is_empty());

    // check that the address_watcher has not received any events
    tokio::select! {
        _ = address_watcher.next() => panic!("Event received unexpectedly!"),
        _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => println!("No event captured, as expected. This test passes."),
    };
}

#[tokio::test]
async fn filter_topics() {
    let (_environment, client) = startup_user_controlled().unwrap();
    let arbiter_token = deploy_arbx(client.clone()).await.unwrap();

    let mut default_watcher = client.watch(&Filter::default()).await.unwrap();
    let mut approval_watcher = client
        .watch(&arbiter_token.approval_filter().filter)
        .await
        .unwrap();

    // Check that both watchers get this event
    let approval = arbiter_token.approve(
        client.default_sender().unwrap(),
        ethers::types::U256::from(TEST_APPROVAL_AMOUNT),
    );
    approval.send().await.unwrap().await.unwrap();
    let default_watcher_event = default_watcher.next().await.unwrap();
    let approval_watcher_event = approval_watcher.next().await.unwrap();
    assert!(!default_watcher_event.data.is_empty());
    assert!(!approval_watcher_event.data.is_empty());
    assert_eq!(default_watcher_event, approval_watcher_event);

    // Check that only the default watcher gets this event
    let mint = arbiter_token.mint(
        ethers::types::H160::from_str(TEST_MINT_TO).unwrap(),
        ethers::types::U256::from(TEST_MINT_AMOUNT),
    );
    mint.send().await.unwrap().await.unwrap();
    let default_watcher_event = default_watcher.next().await.unwrap();
    assert!(!default_watcher_event.data.is_empty());

    // check that the approval_watcher has not received any events
    tokio::select! {
        _ = approval_watcher.next() => panic!("Event received unexpectedly!"),
        _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => println!("No event captured, as expected. This test passes."),
    };
}

#[tokio::test]
async fn block_update_receipt() {
    let (_environment, client) = startup_user_controlled().unwrap();
    let arbiter_token = deploy_arbx(client.clone()).await.unwrap();
    let receipt = arbiter_token
        .mint(client.default_sender().unwrap(), 1000u64.into())
        .send()
        .await
        .unwrap()
        .await
        .unwrap()
        .unwrap();

    assert_eq!(receipt.block_number.unwrap(), 0u64.into());
    let receipt = client.update_block(3, 100).unwrap();
    assert_eq!(receipt.block_number, 3.into());
    assert_eq!(
        receipt.cumulative_gas_per_block,
        revm::primitives::U256::ZERO
    );
    assert_eq!(receipt.transaction_index, 0.into());
}

#[tokio::test]
async fn get_block_number() {
    let (_environment, client) = startup_user_controlled().unwrap();
    let block_number = client.get_block_number().await.unwrap();
    assert_eq!(block_number.as_u64(), 0_u64)
}

#[tokio::test]
async fn get_block_timestamp() {
    let (_environment, client) = startup_user_controlled().unwrap();
    let block_timestamp = client.get_block_timestamp().await.unwrap();
    assert_eq!(block_timestamp, ethers::types::U256::from(1))
}

#[tokio::test]
async fn get_gas_price_user_controlled() {
    // User controlled should have 0 gas price initially
    let (_environment, client) = startup_user_controlled().unwrap();
    let gas_price = client.get_gas_price().await.unwrap();
    assert_eq!(gas_price, ethers::types::U256::from(0));
}

#[tokio::test]
async fn deal() {
    let (_environment, client) = startup_user_controlled().unwrap();
    client
        .apply_cheatcode(Cheatcodes::Deal {
            address: client.default_sender().unwrap(),
            amount: ethers::types::U256::from(1),
        })
        .await
        .unwrap();
    let balance = client.get_balance(client.address(), None).await;
    assert_eq!(balance.unwrap(), 1.into());
}

#[tokio::test]
async fn deal_missing_account() {
    let (_environment, client) = startup_user_controlled().unwrap();
    client
        .apply_cheatcode(Cheatcodes::Deal {
            address: client.default_sender().unwrap(),
            amount: ethers::types::U256::from(1),
        })
        .await
        .unwrap();
    let mut wrong_address = client.address().0;
    wrong_address[0] = wrong_address[0].wrapping_add(1);
    let wrong_address = Address::from(wrong_address);
    let balance = client.get_balance(wrong_address, None).await;
    assert!(balance.is_err());
}

#[tokio::test]
async fn set_gas_price() {
    let (_environment, client) = startup_user_controlled().unwrap();
    assert_eq!(
        client.get_gas_price().await.unwrap(),
        ethers::types::U256::from(0)
    );
    let test_gas_price = ethers::types::U256::from(1337);
    client.set_gas_price(test_gas_price).await.unwrap();
    assert_eq!(client.get_gas_price().await.unwrap(), test_gas_price);
}

#[tokio::test]
async fn get_transaction_count() {
    let (_environment, client) = startup_user_controlled().unwrap();
    let nonce = client
        .get_transaction_count(client.address(), Some(0.into()))
        .await
        .unwrap();
    assert_eq!(nonce.as_u64(), 0_u64);

    let _arbiter_token = deploy_arbx(client.clone()).await.unwrap();
    let nonce_after_tx = client
        .get_transaction_count(client.address(), Some(0.into()))
        .await
        .unwrap();
    assert_eq!(nonce_after_tx.as_u64(), 1_u64);
}

#[tokio::test]
async fn create_nonce_middleware() {
    let (_environment, client) = startup_user_controlled().unwrap();
    let nonce_middleware = NonceManagerMiddleware::new(client.clone(), client.address());
    let nonce = nonce_middleware.initialize_nonce(None).await.unwrap();
    assert_eq!(nonce, 0.into());
}

#[tokio::test]
async fn next_nonce_middleware() {
    let (_environment, client) = startup_user_controlled().unwrap();
    let nonce_middleware = NonceManagerMiddleware::new(client.clone(), client.address());

    let next_nonce = nonce_middleware.next();
    assert_eq!(next_nonce, 0.into());

    let _arbiter_token = deploy_arbx(client.clone()).await.unwrap();
    let next_nonce = nonce_middleware.next();
    assert_eq!(next_nonce.as_u64(), 1_u64);
}

#[tokio::test]
async fn with_environment_nonce_middleware() {
    let (_environment, client) = startup_user_controlled().unwrap();
    let nonce_middleware = NonceManagerMiddleware::new(client.clone(), client.address());

    let nonce = nonce_middleware.initialize_nonce(None).await.unwrap();
    assert_eq!(nonce, 0.into());
}

#[tokio::test]
async fn inner_nonce_environment() {
    let (_environment, client) = startup_user_controlled().unwrap();
    let nonce_middleware = NonceManagerMiddleware::new(client.clone(), client.address());

    let inner = nonce_middleware.inner();
    assert_eq!(inner.address(), client.address());
}

#[tokio::test]
async fn fill_transaction() {
    let (_environment, client) = startup_user_controlled().unwrap();
    let mut tx = TypedTransaction::Eip1559(Default::default());

    assert!(tx.from().is_none());
    assert!(tx.gas_price().is_none());
    client.fill_transaction(&mut tx, None).await.unwrap();
    assert!(tx.from().is_some());
    assert!(tx.gas_price().is_some());
}

#[tokio::test]
async fn fill_transaction_nonce_middleware() {
    let (_environment, client) = startup_user_controlled().unwrap();
    let nonce_middleware = NonceManagerMiddleware::new(client.clone(), client.address());

    let mut tx = TypedTransaction::Eip1559(Default::default());

    assert!(tx.nonce().is_none());
    nonce_middleware
        .fill_transaction(&mut tx, None)
        .await
        .unwrap();
    assert!(tx.nonce().is_some());
}

#[tokio::test]
async fn send_nonce_middleware() {
    let (_environment, client) = startup_user_controlled().unwrap();
    let nonce_middleware = NonceManagerMiddleware::new(client.clone(), client.address());
    let tx = ArbiterToken::deploy(
        client,
        (
            ARBITER_TOKEN_X_NAME.to_string(),
            ARBITER_TOKEN_X_SYMBOL.to_string(),
            ARBITER_TOKEN_X_DECIMALS,
        ),
    )
    .unwrap()
    .deployer
    .tx;
    let receipt = nonce_middleware
        .send_transaction(tx, None)
        .await
        .unwrap()
        .await
        .unwrap()
        .unwrap();
    println!("receipt: {:#?}", receipt);
    let status = receipt.status.unwrap();
    assert_eq!(status, 1.into());
    assert_eq!(receipt.to, None);
}

#[tokio::test]
async fn test_cheatcodes_store() {
    let (_environment, client) = startup_randomly_sampled().unwrap();
    // Get the initial storage and assert it is zero.
    let storage = client
        .get_storage_at(client.address(), ethers::types::H256::zero(), None)
        .await
        .unwrap();
    assert_eq!(storage, ethers::types::H256::zero());

    // Store a random value at the zero storage slot.
    let random_value: ethers::types::H256 = ethers::types::H256::random();
    client
        .apply_cheatcode(Cheatcodes::Store {
            account: client.address(),
            key: ethers::types::H256::zero(),
            value: random_value,
        })
        .await
        .unwrap();

    // Get the account's storage after calling `store`.
    let storage = client
        .get_storage_at(client.address(), ethers::types::H256::zero(), None)
        .await
        .unwrap();

    // Assert that the storage is equal to the random value.
    assert_eq!(storage, random_value);
}

#[tokio::test]
async fn unimplemented_middleware_instruction() {
    let (_environment, client) = startup_user_controlled().unwrap();

    // This method is not implemented and likely never will, so it works to test
    // what happens when we send an unimplemented instruction. We should get a
    // "this method is not yet implemented" error.
    let should_be_error = client.client_version().await;
    assert!(should_be_error.is_err());
    if let crate::middleware::errors::RevmMiddlewareError::Provider(e) =
        should_be_error.unwrap_err()
    {
        assert_eq!(e.to_string(), ProviderError::UnsupportedRPC.to_string());
    } else {
        panic!("Expected RevmMiddlewareError::Provider");
    }
}
