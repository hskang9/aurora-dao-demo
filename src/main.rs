use aurora_workspace::{types::{output::TransactionStatus, Account, KeyType, SecretKey}, EvmContract};
use aurora_workspace_demo::common;
use ethabi::Constructor;
use ethereum_tx_sign::{LegacyTransaction, Transaction};
use serde_json::json;
use std::{fs::File, str::FromStr};
use workspaces::AccountId;

const ETH_RANDOM_HEX_PATH: &str = "./res/Random.hex";
const ETH_RANDOM_ABI_PATH: &str = "./res/Random.abi";
const PRIVATE_KEY: [u8; 32] = [88u8; 32];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Create a sandbox environment.
    let worker = workspaces::sandbox().await?;

    // Deploy Spunik DAO factory contract in sandbox
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
    // Init daofactory contract
    let init_tx = dao_factory
        .call("new")
        .gas(100000000000000)
        .transact()
        .await?;
    println!("{:?}", init_tx);

    // 2. Define parameters of new dao

    // - Define a council
    let bob = common::create_account(&worker, "bob.test.near", None).await?;
    let alice = common::create_account(&worker, "alice.test.near", None).await?;
    let council = ["bob.test.near", "alice.test.near"];

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
    let mint_near1 = a
        .transfer_near(&aurora_dao_id, 10000000000000000000000000)
        .await?;
    let mint_near2 = a
        .transfer_near(&bob.id(), 10000000000000000000000000)
        .await?;
    let mint_near3 = a
        .transfer_near(&alice.id(), 10000000000000000000000000)
        .await?;

    // - Get someone to add store blob for aurora deployment code (aurora-testnet.wasm)
    // get worker account more balance
    let aurora_wasm = std::fs::read("res/aurora-testnet.wasm")?;

    let store_blob = bob
        .call(&dao_contract.id(), "store_blob")
        .args(aurora_wasm)
        .deposit(9534940000000000000000000)
        .gas(100054768750000)
        .transact()
        .await?;
    println!("{:?}", store_blob);

    // - Add proposal to upgrade aurora contract remotely
    println!("Add Proposal");
    let add_upgrade_proposal = bob
        .call(&dao_contract.id(), "add_proposal")
        .args_json(json!({
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
        }))
        .deposit(10u128.pow(24))
        .transact()
        .await?;
    println!("{:?}", add_upgrade_proposal);

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

    // - Proposal is finalized as all council vote yes, so check if precompile works in aurora.test.near!
    // Import Deployed Aurora contract
    let evm: EvmContract = worker
    .import_contract(&AccountId::from_str("aurora.test.near")?, &worker)
    .transact()
    .await?.into();

    // Set the contract.
    let contract = {
        let abi = File::open(ETH_RANDOM_ABI_PATH)?;
        let code = hex::decode(std::fs::read(ETH_RANDOM_HEX_PATH)?)?;
        EthContract::new(abi, code)
    };

    // Create a deploy transaction and sign it.
    let signed_deploy_tx = {
        let deploy_tx = contract.deploy_transaction(0, &[]);
        let ecdsa = deploy_tx.ecdsa(&PRIVATE_KEY).unwrap();
        deploy_tx.sign(&ecdsa)
    };

    // Submit the transaction and get the ETH address.
    let address = match evm
        .as_account()
        .submit(signed_deploy_tx)
        .max_gas()
        .transact()
        .await?
        .into_value()
        .into_result()?
    {
        TransactionStatus::Succeed(bytes) => {
            let mut address_bytes = [0u8; 20];
            address_bytes.copy_from_slice(&bytes);
            address_bytes
        }
        _ => panic!("Ahhhhhh"),
    };
    let random_contract = Random::new(contract, address);

    // Fast forward a few blocks...
    worker.fast_forward(10).await?;

    // Create a call to the Random contract and loop!
    for x in 1..21 {
        let random_tx = random_contract.random_seed_transaction(x);
        let ecdsa = random_tx.ecdsa(&PRIVATE_KEY).unwrap();
        let signed_random_tx = random_tx.sign(&ecdsa);
        if let TransactionStatus::Succeed(bytes) = evm
            .as_account()
            .submit(signed_random_tx)
            .max_gas()
            .transact()
            .await?
            .into_value()
            .into_result()?
        {
            println!("RANDOM SEED: {}", hex::encode(bytes));
        };
        worker.fast_forward(10).await?;
    }
    Ok(())
}

struct Random {
    contract: EthContract,
    address: [u8; 20],
}

impl Random {
    pub fn new(contract: EthContract, address: [u8; 20]) -> Self {
        Self { contract, address }
    }

    pub fn random_seed_transaction(&self, nonce: u128) -> LegacyTransaction {
        let data = self
            .contract
            .abi
            .function("randomSeed")
            .unwrap()
            .encode_input(&[])
            .unwrap();

        LegacyTransaction {
            chain: 1313161556,
            nonce,
            gas_price: Default::default(),
            to: Some(self.address),
            value: Default::default(),
            data,
            gas: u64::MAX as u128,
        }
    }
}

struct EthContract {
    abi: ethabi::Contract,
    code: Vec<u8>,
}

impl EthContract {
    pub fn new(abi_file: File, code: Vec<u8>) -> Self {
        Self {
            abi: ethabi::Contract::load(abi_file).unwrap(),
            code,
        }
    }

    pub fn deploy_transaction(&self, nonce: u128, args: &[ethabi::Token]) -> LegacyTransaction {
        let data = self
            .abi
            .constructor()
            .unwrap_or(&Constructor { inputs: vec![] })
            .encode_input(self.code.clone(), args)
            .unwrap();

        LegacyTransaction {
            chain: 1313161556,
            nonce,
            gas_price: Default::default(),
            to: None,
            value: Default::default(),
            data,
            gas: u64::MAX as u128,
        }
    }
}
