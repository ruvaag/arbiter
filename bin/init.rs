use std::fs;
use std::io::Write;
use std::path::Path;

pub(crate) fn create_simulation(simulation_name: &str) -> std::io::Result<()> {
    let main = r#"
    mod simulations;

    fn main() { 
        let _ = simulations::testsim::run();
    }"#;

    let toml = format!(
        r#"[package]
name = "arbitersim"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "{}"
path = "arbiter/src/main.rs"

# See more keys and their definitions at https://doc.rust-lang.org/cargo/reference/manifest.html
[dependencies]
simulate = {{ git = "https://github.com/primitivefinance/arbiter", package = "simulate" }}"#,
        simulation_name,
    );

    let mod_rs = r#"
    use std::error::Error;

    pub fn run() -> Result<(), Box<dyn Error>> {
        todo!()
    }"#;

    let startup = r#"pub(crate) fn run(manager: &mut SimulationManager) -> Result<(), Box<dyn Error>> {
        let weth_address = manager.deployed_contracts.get("weth").unwrap().address;
        deploy_contracts(manager, weth_address)?;
        let liquid_exchange_xy = manager
            .deployed_contracts
            .get("liquid_exchange_xy")
            .unwrap();
        let address = B160::from_low_u64_be(2);
        let event_filters = vec![SimulationEventFilter::new(
            liquid_exchange_xy,
            "PriceChange",
        )];
        let arbitrageur = SimpleArbitrageur::new(
            "arbitrageur",
            event_filters,
            U256::from(997_000_000_000_000_000u128).into(),
        );
        manager
            .activate_agent(AgentType::SimpleArbitrageur(arbitrageur), address)
            .unwrap();
    
        mint(
            &manager.deployed_contracts,
            manager.agents.get("admin").unwrap(),
            manager.agents.get("arbitrageur").unwrap(),
        )?;
        approve(
            manager.agents.get("admin").unwrap(),
            manager.agents.get("arbitrageur").unwrap(),
            &manager.deployed_contracts,
        )?;
    
        allocate(
            manager.agents.get("admin").unwrap(),
            &manager.deployed_contracts,
        )?;
    
        Ok()
    }
    pub fn deploy() {
    todo!()
    }
    
    pub fn mint() {
    todo!()
    }

    pub fn approve() {
    todo!()
    }

    pub fn allocate() {
    todo!()
    }
    "#;
    // Create a directory
    fs::create_dir_all("arbiter")?;

    // Create a subdirectory

    let src_path = Path::new("arbiter").join("src");
    fs::create_dir_all(&src_path)?;

    let bindings_path = src_path.join("bindings");
    fs::create_dir_all(bindings_path)?;

    let simulations_path = src_path.join("simulations");
    fs::create_dir_all(&simulations_path)?;

    let sim = simulations_path.join(simulation_name);
    fs::create_dir_all(&sim)?;

    // Create a file in the subdirectory
    let file_path = Path::new(".").join("Cargo.toml");
    let mut file = fs::File::create(file_path)?;
    write!(file, "{}", toml)?;

    let file_path = simulations_path.join("mod.rs");
    let mut file = fs::File::create(file_path)?;
    write!(file, "pub mod {};", simulation_name)?;

    let file_path = sim.join("mod.rs");
    let mut file = fs::File::create(file_path)?;
    write!(file, "{}", mod_rs)?;

    let file_path = sim.join("startup.rs");
    let mut file = fs::File::create(file_path)?;
    write!(file, "{}", startup)?;

    let file_path = sim.join("arbitrage.rs");
    fs::File::create(file_path)?;

    let file_path = src_path.join("main.rs");
    let mut file = fs::File::create(file_path)?;
    write!(file, "{}", main)?;

    Ok(())
}

#[test]
fn main() {
    create_simulation("portfolio").unwrap();
}
