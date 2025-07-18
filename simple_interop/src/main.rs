use alloy::hex::FromHex;
use alloy::primitives::{Address, FixedBytes, Log, U64, U256};
use alloy::providers::Provider;
use alloy::sol;
use alloy::sol_types::SolEvent;
use alloy::{primitives::address, providers::ProviderBuilder};
use clap::{Parser, Subcommand};
use std::error::Error;

#[derive(Debug, Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Shows the interop message on the target chain.
    ShowInteropMessage {
        #[arg(long)]
        source_rpc: String,
        #[arg(long)]
        source_tx: String,
    },
    /// Proves the interop message on the target chain.
    ProveInteropMessage {
        #[arg(long)]
        source_rpc: String,
        #[arg(long)]
        source_tx: String,
        #[arg(long)]
        message_index: Option<u64>,
        #[arg(long)]
        target_rpc: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let opts = Cli::parse();
    match opts.command {
        Command::ShowInteropMessage {
            source_rpc,
            source_tx,
        } => show_interop_message(source_rpc, source_tx).await,

        Command::ProveInteropMessage {
            source_rpc,
            source_tx,
            message_index,
            target_rpc,
        } => prove_interop_message(source_rpc, source_tx, message_index, target_rpc).await,
    }
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JsonResponse<T> {
    pub jsonrpc: String,
    pub id: u32,
    pub result: T,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct L2ToL1LogProof {
    pub proof: Vec<String>,
    pub id: u32,
    pub root: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TxInfo {
    #[serde(rename = "blockNumber")]
    pub block_number: U64,
    #[serde(rename = "transactionIndex")]
    pub transaction_index: U64,
    pub from: Address,
    #[serde(rename = "chainId")]
    pub chain_id: U256,
    #[serde(rename = "l1BatchNumber")]
    pub l1_batch_number: U256,
    #[serde(rename = "l1BatchTxIndex")]
    pub l1_batch_tx_index: U256,
}

pub async fn show_interop_message(
    source_rpc: String,
    source_tx: String,
) -> Result<(), Box<dyn Error>> {
    println!(
        "Showing interop message from {} tx: {}",
        source_rpc, source_tx
    );

    let l1_messages = get_interop_messages(source_rpc, source_tx).await?;
    println!("Got {} L1MessageSent events", l1_messages.len());
    for (id, event) in l1_messages.iter().enumerate() {
        println!(
            "L1MessageSent event {}: sender: {}, hash: {}, message: {}",
            id, event._sender, event._hash, event._message
        );
    }

    Ok(())
}

async fn get_interop_messages(
    source_rpc: String,
    source_tx: String,
) -> Result<Vec<L1MessageSent>, Box<dyn Error>> {
    let source_provider = ProviderBuilder::new().connect(&source_rpc).await.unwrap();

    let tx_receipt = source_provider
        .get_transaction_receipt(FixedBytes::from_hex(source_tx).unwrap())
        .await
        .unwrap()
        .unwrap();

    let l1_messages = tx_receipt
        .logs()
        .iter()
        .filter_map(|log| {
            let aa = log;

            let bb = Log {
                address: aa.address(),
                data: aa.data().clone(),
            };

            if let Ok(event) = L1MessageSent::decode_log(&bb) {
                Some(event.data)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    Ok(l1_messages)
}

pub async fn prove_interop_message(
    source_rpc: String,
    source_tx: String,
    message_index: Option<u64>,
    target_rpc: String,
) -> Result<(), Box<dyn Error>> {
    // Here you would implement the logic to prove the interop message
    // This is a placeholder for the actual implementation
    println!(
        "Proving interop message from {} (tx: {}, message: {}) to {}",
        source_rpc,
        source_tx,
        message_index.unwrap_or(0),
        target_rpc
    );

    let interop_messages = get_interop_messages(source_rpc.clone(), source_tx.clone()).await?;
    if interop_messages.is_empty() {
        return Err(Box::from("No interop messages found in the transaction"));
    }
    if interop_messages.len() > 1 && message_index.is_none() {
        return Err(Box::from(
            "Multiple interop messages found, please specify a message index",
        ));
    }
    let interop_message = interop_messages
        .get(message_index.unwrap_or(0) as usize)
        .unwrap();

    let proof = fetch_proof(&source_rpc, &source_tx, None).await?;

    let tx_details = fetch_tx_details(&source_rpc, &source_tx).await?;

    let provider = ProviderBuilder::new().connect(&target_rpc).await.unwrap();
    let contract = IMessageVerification::new(
        address!("0x0000000000000000000000000000000000010009"),
        provider.clone(),
    );

    let proof_bytes = proof
        .proof
        .iter()
        .map(FixedBytes::from_hex)
        .collect::<Result<Vec<_>, _>>()?;

    let l2_message = L2Message {
        txNumberInBatch: 0, // TODO
        sender: interop_message._sender,
        data: interop_message._message.clone(),
    };

    let result = contract
        .proveL2MessageInclusionShared(
            tx_details.chain_id,
            tx_details.l1_batch_number,
            tx_details.l1_batch_tx_index,
            l2_message,
            proof_bytes,
        )
        .call()
        .await
        .unwrap();

    if result {
        println!("\x1b[32mResult: true\x1b[0m");
    } else {
        println!("\x1b[31mResult: false\x1b[0m");
    }

    Ok(())
}

sol! {
    struct L2Message {
        uint16 txNumberInBatch;
        address sender;
        bytes data;
    }

    #[sol(rpc)]
    contract IMessageVerification {
        function proveL2MessageInclusionShared(
            uint256 _chainId,
            uint256 _blockOrBatchNumber,
            uint256 _index,
            L2Message calldata _message,
            bytes32[] calldata _proof
        ) external view returns (bool);
    }

    event L1MessageSent(address indexed _sender, bytes32 indexed _hash, bytes _message);

}

pub async fn fetch_proof(
    rpc_url: &str,
    tx_hash: &str,
    index: Option<usize>,
) -> Result<L2ToL1LogProof, Box<dyn Error>> {
    let client = reqwest::Client::new();
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "zks_getL2ToL1LogProof",
        "params": [
            tx_hash,
            index,
            "proof_based_gw"
        ]
    });

    let res = client
        .post(rpc_url)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await?;

    if res.status().is_success() {
        let proof: JsonResponse<L2ToL1LogProof> = res.json().await?;
        Ok(proof.result)
    } else {
        Err(Box::from(format!(
            "Failed to fetch proof: {}",
            res.status()
        )))
    }
}

pub async fn fetch_tx_details(rpc_url: &str, tx_hash: &str) -> Result<TxInfo, Box<dyn Error>> {
    let client = reqwest::Client::new();
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_getTransactionByHash",
        "params": [tx_hash]
    });

    let res = client
        .post(rpc_url)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await?;

    if res.status().is_success() {
        let tx: JsonResponse<TxInfo> = res.json().await?;
        Ok(tx.result)
    } else {
        Err(Box::from(format!("Failed to fetch tx: {}", res.status())))
    }
}
