use aurora_workspace::types::{output::TransactionStatus, SecretKey, KeyType, Account};
use aurora_workspace_demo::common;
use ethabi::Constructor;
use ethereum_tx_sign::{LegacyTransaction, Transaction};
use near_units;
use serde_json::json;
use std::{fs::File, str::FromStr};
use workspaces::AccountId;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Create a sandbox environment.
    let worker = workspaces::sandbox().await?;

    // Deploy Spunik DAO factory contract in sandbox
    let wasm = std::fs::read("res/sputnikdao_factory2.wasm")?;
    let dao_factory = worker.create_tla_and_deploy(AccountId::from_str("dao-factory.test.near")?, SecretKey::from_random(KeyType::ED25519), &wasm).await?.unwrap();
    println!("Contract Id: {}", dao_factory.id());
    // Init daofactory contract
    let init_tx = dao_factory
        .call("new")
        .gas(100000000000000)
        .transact()
        .await?;
    println!("{:?}", init_tx);

    // 2. Define parameters of new dao

    // - Define a council
    let bob = common::create_account(&worker, "bob.test.near", None);
    let alice = common::create_account(&worker, "alice.test.near", None);
    let council = ["test.near", "bob.test.near", "alice.test.near"];

    // - Configure name, purpose, and initial council members of the DAO and convert the arguments in base64
    let args = json!({
        "config": {
            "name": "aurora-dao",
            "purpose": "Aurora internal test DAO",
            "metadata": "",
        },
        "policy": ["test.near", "bob.test.near",  "alice.test.near"],
    });
    let args_bs64 = base64::encode(&serde_json::to_vec(&args).unwrap());

    // - Create a new DAO
    let create_new_dao = dao_factory
        .call("create")
        .args_json(json!({
            "name": "aurora-dao",
            "args": format!("{}", args_bs64),
        }))
        .deposit(10000000000000000000000000)
        .gas(150000000000000)
        .transact()
        .await?;

    println!("{:?}", create_new_dao);

    // 3. Get the council deploy contract from dao
    let aurora_dao_id = AccountId::from_str(&format!("aurora-dao.{}", dao_factory.id()))?;

    println!("Aurora DAO ID: {}", aurora_dao_id);
    let dao_contract = worker
        .import_contract(&aurora_dao_id, &worker)
        .transact()
        .await?;

    // - Get policy
    let get_policy = dao_contract.view("get_policy").await?;
    println!("{:?}", get_policy);
    let a = worker.root_account()?;
    let mint_near = a.transfer_near(&aurora_dao_id, 10000000000000000000000000).await?;
    println!("{:?}", mint_near);

    // - Get someone to add store blob for aurora deployment code (aurora-testnet.wasm)
    // get worker account more balance
    let aurora_wasm = std::fs::read("res/aurora-testnet.wasm")?;
    let store_blob = dao_contract.call("store_blob").args(aurora_wasm).deposit(9534940000000000000000000).gas(100054768750000).transact().await?;
    println!("{:?}", store_blob);
    
    // - Add proposal to upgrade aurora contract remotely
    let add_upgrade_proposal = dao_contract.call("add_proposal").args_json(json!({
        "proposal": {
          "description": "Upgrade Aurora contract",
          "kind": {
            "UpgradeRemote": {
              "receiver_id": "aurora.test.near",
              "method_name": "migrate",
              "hash": "HN2fH5y6mbBkvgsazk8qZtqwxEU6ykLPZjz3xNnmuVcG",
              "role": "council"
            }
          }
        }
      })).deposit(10u128.pow(24)).transact().await?;
    println!("{:?}", add_upgrade_proposal);

    // - Approve Proposal then finalize proposal
    let approve_proposal = dao_contract.call("vote").args_json(json!({
        "proposal_id": 0,
        "vote": {
          "Approve": {}
        }
      })).transact().await?;

    // - Check if precompile works in aurora.test.near!
    // TODO: add codes from main2.rs

    Ok(())
}
