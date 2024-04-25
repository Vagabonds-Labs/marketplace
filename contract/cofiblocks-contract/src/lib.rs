mod u256;

use std::{path::PathBuf, str::FromStr, sync::Arc};

use cainome_cairo_serde::{ByteArray, CairoSerde};
use serde::Deserialize;
use starknet::{
    accounts::{ExecutionEncoding, SingleOwnerAccount},
    contract::ContractFactory,
    core::{
        chain_id,
        types::{BlockId, BlockTag, ExecutionResult, FieldElement, FunctionCall, StarknetError},
        utils::get_selector_from_name,
    },
    macros::{felt, short_string},
    providers::{jsonrpc::HttpTransport, JsonRpcClient, Provider, ProviderError, Url},
    signers::{LocalWallet, SigningKey},
};
use thiserror::Error;
use tokio::time::Duration;

use crate::u256::U256;

/// Contract parameters that needs to be passed for contract creation
#[derive(Debug, Deserialize)]
pub struct ContractSpec {
    /// Base URI for the Contract
    pub base_uri: String,
    /// Tokens information
    pub tokens: Vec<ContractTokensInfo>,
}

/// Token information needed for contract creation
#[derive(Debug, Deserialize)]
pub struct ContractTokensInfo {
    /// Token name
    pub name: String,
    /// Token value
    pub value: u64,
}

/// Supported Starknet networks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum Network {
    /// Mainnet network
    Mainnet,
    /// Sepolia network
    #[default]
    Sepolia,
}

/// Errors that the client can return
#[derive(Debug, Error)]
enum ClientError {
    /// Reverted transaction
    #[error("Transaction reverted, reason: {0}")]
    TransactionReverted(String),
    /// Provider error
    #[error("StarknetError: {0}")]
    Provider(#[from] ProviderError),
}

/// Utility function to resolve the ERC1155 class hash
fn class_hash(network: &Network) -> FieldElement {
    match network {
        Network::Mainnet => panic!("Network not supported yet"),
        Network::Sepolia => {
            felt!("0x0120d1f2225704b003e77077b8507907d2a84239bef5e0abb67462495edd644f")
        }
    }
}

/// Utility function to resolve the client
fn client(network: &Network) -> Arc<JsonRpcClient<HttpTransport>> {
    let client = match network {
        Network::Mainnet => panic!("Network not supported yet"),
        Network::Sepolia => JsonRpcClient::new(HttpTransport::new(
            Url::parse("https://starknet-sepolia.public.blastapi.io/rpc/v0_6").unwrap(),
        )),
    };
    Arc::new(client)
}

/// Utility function to resolve the chain ID
fn chain_id(network: &Network) -> FieldElement {
    match network {
        Network::Mainnet => chain_id::MAINNET,
        Network::Sepolia => short_string!("SN_SEPOLIA"),
    }
}

/// Utility to resolve a keystore from a path
fn resolve_keystore(path: &PathBuf) -> LocalWallet {
    let keystore = PathBuf::from(path);

    if !keystore.exists() {
        panic!("keystore file not found");
    }

    let password = rpassword::prompt_password("Enter keystore password: ").unwrap();

    let key = SigningKey::from_keystore(keystore, &password).unwrap();
    LocalWallet::from_signing_key(key)
}

/// Utility function to watch for a transaction
async fn watch_tx(
    client: Arc<JsonRpcClient<HttpTransport>>,
    transaction_hash: FieldElement,
    poll_interval: Duration,
) -> Result<(), ClientError> {
    loop {
        match client.get_transaction_receipt(transaction_hash).await {
            Ok(receipt) => match receipt.execution_result() {
                ExecutionResult::Succeeded => {
                    eprintln!(
                        "Transaction {} confirmed",
                        format!("{:#064x}", transaction_hash)
                    );

                    return Ok(());
                }
                ExecutionResult::Reverted { reason } => {
                    return Err(ClientError::TransactionReverted(reason.to_string()));
                }
            },
            Err(ProviderError::StarknetError(StarknetError::TransactionHashNotFound)) => {
                eprintln!("Transaction not confirmed yet...");
            }
            Err(err) => return Err(err.into()),
        }

        tokio::time::sleep(poll_interval).await;
    }
}

fn tokens_to_felts(token_names: &Vec<String>) -> Vec<FieldElement> {
    let mut tokens = vec![];
    for token_name in token_names {
        let mut bytes = [0u8; 32];
        hex::decode_to_slice(token_name.clone(), &mut bytes as &mut [u8]).unwrap();
        let value = U256 {
            high: u128::from_be_bytes(bytes[16..].try_into().unwrap()),
            low: u128::from_be_bytes(bytes[..16].try_into().unwrap()),
        };
        tokens.push(value);
    }
    Vec::<U256>::cairo_serialize(&tokens)
}

/// Deploys a ERC-1155 contract to the specified network, using an account address, a keystore
/// path, a recipient and a contract spec.
pub async fn deploy_contract(
    network: &Network,
    address: &str,
    keystore_path: &PathBuf,
    recipient: &str,
    spec: &ContractSpec,
) {
    let client = client(network);
    let class_hash = class_hash(network);
    let signer = resolve_keystore(keystore_path);

    let mut account = SingleOwnerAccount::new(
        client.clone(),
        signer,
        FieldElement::from_str(address).unwrap(),
        chain_id(network),
        ExecutionEncoding::New,
    );

    // `SingleOwnerAccount` defaults to checking nonce and estimating fees against the latest
    // block. Optionally change the target block to pending with the following line:
    account.set_block_id(BlockId::Tag(BlockTag::Pending));

    // Wrapping in `Arc` is meaningless here. It's just showcasing it could be done as
    // `Arc<Account>` implements `Account` too.
    let account = Arc::new(account);

    let contract_factory = ContractFactory::new(class_hash, account);
    let salt = SigningKey::from_random().secret_scalar();
    let mut ctor_args = vec![];

    // Create the constructor arguments
    let byte_array = ByteArray::from_string(&spec.base_uri).unwrap();
    ctor_args.append(&mut ByteArray::cairo_serialize(&byte_array));
    ctor_args.push(FieldElement::from_hex_be(recipient).unwrap());

    ctor_args.append(&mut tokens_to_felts(
        &spec
            .tokens
            .iter()
            .map(|token_info| token_info.name.clone())
            .collect(),
    ));

    let mut values = vec![];
    for token in &spec.tokens {
        let value = U256 {
            high: 0,
            low: token.value as u128,
        };
        values.push(value);
    }
    ctor_args.append(&mut Vec::<U256>::cairo_serialize(&values));

    let contract_deployment = contract_factory
        .deploy(ctor_args, salt, true)
        .max_fee(FieldElement::from(400000000000000_u128)); // Fixme, what value is suitable?
    let deployed_address = contract_deployment.deployed_address();
    let estimated_fee = contract_deployment.estimate_fee().await.unwrap();
    eprintln!(
        "Deploying class {} with salt {}, estimated fee {}...",
        format!("{:#064x}", class_hash),
        format!("{:#064x}", salt),
        format!("{:#064x}", estimated_fee.overall_fee)
    );
    eprintln!(
        "The contract will be deployed at address {}",
        format!("{:#064x}", deployed_address)
    );

    let deployment_tx = contract_deployment.send().await.unwrap().transaction_hash;
    eprintln!(
        "Contract deployment transaction: {}",
        format!("{:#064x}", deployment_tx)
    );
    eprintln!(
        "Waiting for transaction {} to confirm...",
        format!("{:#064x}", deployment_tx)
    );
    watch_tx(client, deployment_tx, Duration::from_millis(1000))
        .await
        .unwrap();
}

/// Shows the account balance for a set of tokens
pub async fn show_contract(
    network: &Network,
    contract_address: &String,
    accounts: &Vec<String>,
    tokens: Vec<String>,
) {
    let client = client(network);
    let contract_address = FieldElement::from_hex_be(contract_address).unwrap();
    let account_felts = accounts
        .iter()
        .map(|account| FieldElement::from_hex_be(account).unwrap())
        .collect();
    let selector = get_selector_from_name("balance_of_batch").unwrap();

    let mut calldata = Vec::<FieldElement>::cairo_serialize(&account_felts);
    calldata.append(&mut tokens_to_felts(&tokens));

    let result = client
        .call(
            FunctionCall {
                contract_address,
                entry_point_selector: selector,
                calldata,
            },
            BlockId::Tag(BlockTag::Pending),
        )
        .await
        .unwrap();

    if result.is_empty() {
        println!("[]");
    } else {
        println!("[");

        for (ind_element, element) in result.iter().enumerate() {
            println!(
                "    \"{:#064x}\"{}",
                element,
                if ind_element == result.len() - 1 {
                    ""
                } else {
                    ","
                }
            );
        }

        println!("]");
    }
}
