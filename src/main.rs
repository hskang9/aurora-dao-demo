use aurora_workspace::types::output::TransactionStatus;
use aurora_workspace_demo::common;
use ethabi::Constructor;
use ethereum_tx_sign::{LegacyTransaction, Transaction};
use std::fs::File;
use serde_json::json;
use near_units;

const PRIVATE_KEY: [u8; 32] = [88u8; 32];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Create a sandbox environment.
    let worker = workspaces::sandbox().await?;
    // Deploy Spunik DAO factory contract in sandbox
    let wasm = std::fs::read("res/sputnikdao_factory2.wasm")?;
    let dao_factory = worker.dev_deploy(&wasm).await?;
    println!("Contract Id: {}", dao_factory.id());
    // Init daofactory contract
    let init_tx = dao_factory.call("new").gas(100000000000000).transact().await?;
    println!("{:?}", init_tx);

    // 2. Define parameters of new dao

    // - Define a council
    let bob = common::create_account(&worker, "bob.near", None);
    let alice = common::create_account(&worker, "alice.near", None);
    let council = ["bob.near", "alice.near"];

    // - Configure name, purpose, and initial council members of the DAO and convert the arguments in base64
    let args = json!({
        "config": {
            "name": "aurora-dao",
            "purpose": "Aurora internal test DAO",
            "metadata": "",
        }, 
        "policy": council,
    });
    let args_bs64 = base64::encode(&serde_json::to_vec(&args).unwrap());

    let create_new_dao = dao_factory.call("create").args_json(json!({
        "name": "aurora-dao",
        "args": args_bs64,
    })).deposit(10000000000000000000000000).gas(150000000000000).transact().await?;
    
    println!("{:?}", create_new_dao);

    Ok(())
}
