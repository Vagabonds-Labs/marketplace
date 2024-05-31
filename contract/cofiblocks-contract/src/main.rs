use clap::{Parser, Subcommand};
use std::path::PathBuf;

use cofiblocks_contract::{deploy_contract, show_contract, ContractSpec, Network};

/// Command line arguments for the binary
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
    network: Option<Network>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(arg_required_else_help = true)]
    ShowAll {
        /// Contract address
        contract_address: String,
        /// The account to show
        account: String,
        /// Shows all tokens in the contract specification
        spec: PathBuf,
    },
    #[command(arg_required_else_help = true)]
    Show {
        /// Contract address
        contract_address: String,
        /// The account to show
        account: String,
        /// Token ID to show
        token: String,
    },
    #[command(arg_required_else_help = true)]
    Deploy {
        /// Creates a contract given a contract specification
        spec: PathBuf,
        /// Signing account address
        address: String,
        /// Keystore path
        keystore: PathBuf,
        /// Recipient of minted tokens
        recipient: String,
    },
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let network = args.network.unwrap_or_default();

    match args.command {
        Commands::ShowAll {
            contract_address,
            account,
            spec,
        } => {
            if !spec.exists() {
                panic!("Spec file not found");
            }
            let file_content = std::fs::read_to_string(spec).unwrap();
            let spec: ContractSpec = toml::from_str(&file_content).unwrap();
            let tokens: Vec<String> = spec
                .tokens
                .iter()
                .map(|token_info| token_info.name.clone())
                .collect();
            show_contract(&network, &contract_address, &[account], &tokens).await
        }
        Commands::Show {
            contract_address,
            account,
            token,
        } => show_contract(&network, &contract_address, &[account], &[token]).await,
        Commands::Deploy {
            spec,
            address,
            keystore,
            recipient,
        } => {
            if !spec.exists() {
                panic!("Spec file not found");
            }
            let file_content = std::fs::read_to_string(spec).unwrap();
            let result = toml::from_str(&file_content);
            println!("{:?}", result);
            let spec = result.unwrap();
            //let spec = toml::from_str(&file_content).unwrap();
            deploy_contract(&network, &address, &keystore, &recipient, &spec).await
        }
    }
}
