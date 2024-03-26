use std::{path::PathBuf, str::FromStr, sync::Arc};

use cainome_cairo_serde::{ByteArray, CairoSerde};
use serde::Deserialize;
use starknet::{
    accounts::{ExecutionEncoding, SingleOwnerAccount},
    contract::ContractFactory,
    core::{
        chain_id,
        types::{BlockId, BlockTag, ExecutionResult, FieldElement, FunctionCall, StarknetError},
        utils::cairo_short_string_to_felt,
    },
    macros::{felt, short_string},
    providers::{jsonrpc::HttpTransport, JsonRpcClient, Provider, ProviderError, Url},
    signers::{LocalWallet, SigningKey},
};
use thiserror::Error;
use tokio::time::Duration;

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
    // For the token span array we need to set the len as a felt first
    ctor_args.push(FieldElement::from(spec.tokens.len()));
    for token in &spec.tokens {
        // FIXME: Hashing is not working
        //let mut hasher = Sha256::new();
        //hasher.update(&token.name);
        //let hashed_name: [u8; 32] = hasher.finalize().into();
        //ctor_args.push(FieldElement::from_bytes_be(&hashed_name).unwrap());
        ctor_args.push(cairo_short_string_to_felt(&token.name).unwrap());
    }
    // For the token span array we need to set the len as a felt first
    ctor_args.push(FieldElement::from(spec.tokens.len()));
    for token in &spec.tokens {
        ctor_args.push(FieldElement::from(token.value));
    }

    let contract_deployment = contract_factory.deploy(ctor_args, salt, true);
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

pub async fn show_contract(network: &Network, account: &str, tokens: Vec<String>) {
    let client = client(network);
    let contract_address = FieldElement::from_hex_be(account).unwrap();
    let selector = FieldElement::from_str("balance_of_batch").unwrap();

    let mut calldata = vec![];
    calldata.push(FieldElement::from(tokens.len()));
    for token in tokens {
        // FIXME: Hashing is not working
        //let mut hasher = Sha256::new();
        //hasher.update(&token);
        //let hashed_name: [u8; 32] = hasher.finalize().into();
        //calldata.push(FieldElement::from_bytes_be(&hashed_name).unwrap());
        calldata.push(cairo_short_string_to_felt(&token).unwrap());
    }

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
