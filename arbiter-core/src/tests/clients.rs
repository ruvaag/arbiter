use super::*;

#[test]
fn simulation_signer() -> Result<()> {
    let (_, client) = startup_user_controlled()?;
    assert_eq!(
        client.address(),
        Address::from_str("0x2efdc9eecfee3a776209fcb8e9a83a6b221d74f5").unwrap()
    );
    Ok(())
}

#[test]
fn multiple_signer_addresses() {
    let mut environment = builder::EnvironmentBuilder::new().build();
    environment.run();
    let client_1 = RevmMiddleware::new(&environment, Some("0")).unwrap();
    let client_2 = RevmMiddleware::new(&environment, Some("1")).unwrap();
    assert_ne!(client_1.address(), client_2.address());
}

#[test]
fn signer_collision() {
    let environment = builder::EnvironmentBuilder::new().build();
    RevmMiddleware::new(&environment, Some("0")).unwrap();
    assert!(RevmMiddleware::new(&environment, Some("0")).is_err());
}
