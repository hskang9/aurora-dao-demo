use aurora_workspace::{
    types::{ KeyType, SecretKey}, EvmContract,
};
use aurora_workspace_demo::common;
use serde_json::json;
use std::{str::FromStr, sync::Arc};
use workspaces::AccountId;
use borsh::{BorshSerialize, BorshDeserialize};



#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Create a sandbox environment.
    let worker = workspaces::sandbox().await?;

    worker.fast_forward(1).await?;

    // 2. deploy the Aurora EVM in sandbox.
    let (evm, sk) =
        common::init_and_deploy_contract_with_path(&worker, "./res/aurora-testnet-2.8.0-set-owner.wasm")
            .await?;

    worker.fast_forward(1).await?;

    
    let version_bf = evm.as_account().version().await?.result;
    println!("Aurora version before upgrade: {:?}", version_bf); 

    // 3. Deploy Spunik DAO factory contract in sandbox
    println!("Deploying Spunik DAO factory contract");
    let wasm = std::fs::read("res/sputnikdao_factory2.wasm")?;
    let dao_factory = worker
        .create_tla_and_deploy(
            AccountId::from_str("dao-factory.test.near")?,
            SecretKey::from_random(KeyType::ED25519),
            &wasm,
        )
        .await?
        .unwrap();
    println!("Contract Id: {}", dao_factory.id());

    worker.fast_forward(1).await?;
    // Init daofactory contract
    let init_tx = dao_factory
        .call("new")
        .gas(100000000000000)
        .transact()
        .await?;
    println!("{:?}", init_tx);

    worker.fast_forward(1).await?;
    // 4. Define parameters of new dao

    // - Define a council
    let bob = common::create_account(&worker, "bob.test.near", None).await?;
    let alice = common::create_account(&worker, "alice.test.near", None).await?;

    // - Configure name, purpose, and initial council members of the DAO and convert the arguments in base64
    let args = json!({
        "config": {
            "name": "aurora-dao",
            "purpose": "Aurora internal test DAO",
            "metadata": "",
        },
        "policy": ["bob.test.near",  "alice.test.near"],
    });
    let args_bs64 = base64::encode(&serde_json::to_vec(&args).unwrap());

    // - Create a new DAO
    println!("Creating new DAO");
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

    worker.fast_forward(1).await?;

    // 5. Get the council deploy contract from dao
    let aurora_dao_id = AccountId::from_str(&format!("aurora-dao.{}", dao_factory.id()))?;

    // 5-1. Owner shift to the new owner of Aurora
    
    println!("Shift owner of Aurora EVM to the new owner");
    let owner = common::create_account(&worker, "owner.test.near", None).await?;

    let args:NewOwnerArgs = NewOwnerArgs {
        owner_id: AccountId::from_str("aurora.test.near").unwrap(),
    };
    let borsh_args = args.try_to_vec().unwrap();
    let set_owner = owner.call(&AccountId::from_str("aurora.test.near").unwrap(), "set_owner").args_borsh(borsh_args).transact().await?;
    let owner = evm.as_account().owner().await?.result;
    println!("EVM owner: {:?}", owner);


    println!("Aurora DAO ID: {}", aurora_dao_id);
    let dao_contract = worker
        .import_contract(&aurora_dao_id, &worker)
        .transact()
        .await?;

    // - Get policy
    let get_policy = dao_contract.view("get_policy").await?;
    // println!("{:?}", get_policy);
    let root = worker.root_account()?;
    root.transfer_near(&aurora_dao_id, 10000000000000000000000000)
        .await?.into_result();
    root.transfer_near(&bob.id(), 10000000000000000000000000)
        .await?.into_result();
    root.transfer_near(&alice.id(), 10000000000000000000000000)
        .await?.into_result();
    worker.fast_forward(1).await?;

    // - Get someone to add store blob for aurora deployment code (aurora-testnet.wasm)
    // get worker account more balance
    let aurora_wasm = std::fs::read("./res/aurora-testnet-2.8.1.wasm")?;

    let store_blob = bob
        .call(&dao_contract.id(), "store_blob")
        .args(aurora_wasm)
        .deposit(9534940000000000000000000)
        .gas(100054768750000)
        .transact()
        .await?;
    // TODO: get result from this execution result type where it keeps result as private
    println!("{:?}", store_blob);

    worker.fast_forward(1).await?;

    // - Add proposal to upgrade aurora contract remotely
    println!("Add staging upgrade Proposal");
    let add_upgrade_proposal = bob
        .call(&dao_contract.id(), "add_proposal")
        .args_json(json!({
          "proposal": {
            "description": "Upgrade Aurora contract",
            "kind": {
              "UpgradeRemote": {
                "receiver_id": "aurora.test.near",
                "method_name": "stage_upgrade",
                "hash": "G4bJiWEnJsktaLueP7ri5sh3VhJBr3L1YjtYvKuCwLSC",
                "role": "council"
              }
            }
          }
        }))
        .deposit(10u128.pow(24))
        .transact()
        .await?;
    println!("{:?}", add_upgrade_proposal);

    worker.fast_forward(1).await?;

    // - Approve Proposal
    println!("Approve Proposal");
    let approve_proposal1 = bob
        .call(&dao_contract.id(), "act_proposal")
        .args_json(json!({
          "id": 0,
          "action": "VoteApprove",
          "memo": ""
        }))
        .gas(10038214819423)
        .transact()
        .await?;
    println!("{:?}", approve_proposal1);

    worker.fast_forward(1).await?;

    let approve_proposal2 = alice
        .call(&dao_contract.id(), "act_proposal")
        .args_json(json!({
          "id": 0,
          "action": "VoteApprove",
          "memo": ""
        }))
        .gas(100_000_000_000_000)
        .transact()
        .await?;
    println!("{:?}", approve_proposal2);

    worker.fast_forward(1).await?;

    // - Proposal is finalized as all council vote yes, so check if precompile works in aurora.test.near!
    // Import Deployed Aurora contract
    let evm_af = EvmContract::from_secret_key("aurora.test.near", sk, &worker)?;
    let version_af = evm_af.as_account().version().await?.result;
    println!("Aurora version after upgrade: {}", version_af);

    Ok(())
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct NewOwnerArgs {
    pub owner_id: AccountId,
}