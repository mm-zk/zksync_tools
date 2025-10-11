use std::{collections::HashMap, str::FromStr};

use alloy::{
    primitives::{Address, B256},
    providers::{Provider, ProviderBuilder},
    rpc::types::Filter,
    sol_types::SolEvent, // for ABI-safe decoding of the commit function
};
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

use crate::{
    chain_genesis::get_genesis_upgrade,
    contracts::BlockCommit,
    state::BlockchainState,
    state_genesis::init_genesis,
    statediffs::BatchInfo,
};

pub mod bytecodes;
pub mod chain_genesis;
pub mod contracts;
pub mod sequencer_db;
pub mod state;
pub mod state_genesis;
pub mod statediffs;
#[derive(Debug, Parser)]
#[command(
    name = "l1-recovery",
    about = "Scan Ethereum L1 and recover zkSync chain state"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]

pub enum Command {
    /// Recover state from L1, check correctness and optionally write to json file.
    Recover(RecoverArgs),
    WriteToDB(WriteToDBArgs),
}

#[derive(Debug, Parser)]
pub struct WriteToDBArgs {
    /// Input file (JSON) with the blockchain state.
    #[arg(long)]
    input: String,

    /// RocksDB path to write the state to.
    #[arg(long)]
    db_path: String,
}

#[derive(Debug, Parser)]
pub struct RecoverArgs {
    /// Ethereum RPC URL (archive preferred)
    #[arg(long)]
    rpc: String,

    /// Diamond Proxy address of the zkSync chain.
    /// Can be omitted if --bridgehub and --chain-id are provided (will be auto-discovered from L1).
    #[arg(long)]
    address: Option<String>,

    /// Bridgehub contract address on L1 (for auto-discovering diamond proxy)
    #[arg(long)]
    bridgehub: Option<String>,

    /// Chain ID (for auto-discovering diamond proxy from bridgehub)
    #[arg(long)]
    chain_id: Option<u64>,

    /// Start block (inclusive). If omitted, uses earliest.
    #[arg(long)]
    from: Option<u64>,

    /// End block (inclusive). If omitted, uses latest.
    #[arg(long)]
    to: Option<u64>,

    /// Chunk size (number of blocks per request)
    #[arg(long, default_value_t = 2_000u64)]
    chunk: u64,

    /// Transaction batch size (number of transactions to fetch concurrently).
    #[arg(long, default_value_t = 1)]
    tx_batch_size: usize,

    /// Event scan concurrency (number of chunks to scan in parallel for events).
    #[arg(long, default_value_t = 1)]
    concurrency: usize,

    /// Output file (JSON). If omitted, prints summary to stdout.
    #[arg(long)]
    output: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();
    let args = Cli::parse();

    match args.command {
        Command::Recover(args) => run_recover(args).await?,
        Command::WriteToDB(args) => write_to_db(args)?,
    }
    Ok(())
}

/// Fetch diamond proxy address by calling getZKChain(uint256) on bridgehub (L1 only)
async fn fetch_diamond_proxy<P: Provider>(
    l1_provider: &P,
    bridgehub: Address,
    chain_id: u64,
) -> Result<Address> {
    use alloy::primitives::{U256, keccak256};

    // Encode the call: getZKChain(uint256)
    // Calculate function selector dynamically: keccak256("getZKChain(uint256)")
    let function_signature = "getZKChain(uint256)";
    let hash = keccak256(function_signature.as_bytes());
    let selector = &hash[0..4]; // First 4 bytes

    tracing::debug!(
        "Function signature: {}, selector: 0x{}",
        function_signature,
        hex::encode(selector)
    );

    let mut calldata = selector.to_vec();

    // Encode chain_id as uint256 (32 bytes, big-endian)
    let chain_id_u256 = U256::from(chain_id);
    let chain_id_bytes = chain_id_u256.to_be_bytes::<32>();
    calldata.extend_from_slice(&chain_id_bytes);

    // Make eth_call
    let call_data = alloy::primitives::Bytes::from(calldata);
    let tx = alloy::rpc::types::TransactionRequest::default()
        .to(bridgehub)
        .input(call_data.into());

    let result = l1_provider
        .call(tx)
        .await
        .context("failed to call getZKChain on bridgehub")?;

    // Result should be a 32-byte address (padded)
    if result.len() < 32 {
        bail!("invalid response from getZKChain: too short");
    }

    // Extract the last 20 bytes (address is right-aligned in 32-byte word)
    let address_bytes = &result[result.len() - 20..];
    let diamond_proxy = Address::from_slice(address_bytes);

    Ok(diamond_proxy)
}

async fn run_recover(args: RecoverArgs) -> Result<()> {
    // Load genesis from file.
    let genesis = init_genesis();

    let provider = ProviderBuilder::new().connect_http(args.rpc.parse()?);

    // Resolve diamond proxy address
    let target = if let Some(address) = args.address {
        Address::from_str(&address)
            .with_context(|| format!("invalid address: {}", address))?
    } else if let (Some(bridgehub_str), Some(chain_id)) = (args.bridgehub, args.chain_id) {
        tracing::info!("Auto-discovering diamond proxy from L1 bridgehub");

        let bridgehub = Address::from_str(&bridgehub_str)
            .with_context(|| format!("invalid bridgehub address: {}", bridgehub_str))?;
        tracing::info!("Bridgehub address: {}", bridgehub);
        tracing::info!("Chain ID: {}", chain_id);

        let diamond_proxy = fetch_diamond_proxy(&provider, bridgehub, chain_id).await?;
        tracing::info!("Diamond proxy address: {}", diamond_proxy);

        diamond_proxy
    } else {
        bail!("Either --address must be provided, or both --bridgehub and --chain-id for auto-discovery");
    };

    // Resolve default bounds
    let latest = provider
        .get_block_number()
        .await
        .context("get latest block")?;
    let from = args.from.unwrap_or(0);
    let to = args.to.unwrap_or(latest);
    if from > to {
        bail!("from block must be <= to block");
    }

    tracing::info!(
        "Scanning {} -> {} ({} blocks) on {}",
        from,
        to,
        to - from + 1,
        args.rpc
    );

    // First - try to find genesis upgrade event (should be somewhere at the beginning).
    let genesis_local_info = get_genesis_upgrade(&provider, target, from, to, args.chunk, args.concurrency).await?;

    // Now we can create initial blockchain state.
    let mut blockchain_state = BlockchainState::new(genesis.clone(), genesis_local_info);

    // Then start scanning for 'CommitBatches' call, and collecting the state.
    let mut start = from;
    let chunks_total = (to - from) / args.chunk + 1;
    tracing::info!("Scanning for CommitBatches events: {} total chunks", chunks_total);
    let mut chunks_done = 0;
    while start <= to {
        chunks_done += 1;
        if chunks_done % 10 == 0 {
            tracing::info!(
                "Commit scan progress: {}/{} chunks ({:.1}%), found {} batches so far",
                chunks_done,
                chunks_total,
                (chunks_done as f64 / chunks_total as f64) * 100.0,
                blockchain_state.current_batch
            );
        }
        let end = (start + args.chunk - 1).min(to);
        let scan_results = get_commit_batches_from_range(&provider, target, start, end, args.tx_batch_size).await?;

        let mut batch_numbers = scan_results.keys().cloned().collect::<Vec<_>>();
        batch_numbers.sort_unstable();

        for batch_number in batch_numbers {
            let batch_info = scan_results.get(&batch_number).unwrap();
            blockchain_state.apply_batch(batch_number, batch_info);
        }

        start = end.saturating_add(1);
    }

    tracing::info!(
        "All {} batches and {} blocks applied successfully, final tree root: 0x{}",
        blockchain_state.current_batch,
        blockchain_state.current_block,
        hex::encode(blockchain_state.tree.compute_root())
    );

    if let Some(output) = args.output {
        let json = serde_json::to_string_pretty(&blockchain_state)
            .context("serialize blockchain state to JSON")?;
        std::fs::write(&output, json).with_context(|| format!("write to {}", output))?;
        tracing::info!("Wrote blockchain state to {}", output);
    }

    Ok(())
}

async fn get_commit_batches_from_range<P: Provider + Clone>(
    provider: &P,
    address: Address,
    from: u64,
    to: u64,
    tx_batch_size: usize,
) -> Result<HashMap<u64, BatchInfo>> {
    // Build a filter: address + topic0 = event signature. Indexed params (batchNumber, batchHash, commitment)
    // can also be filtered later via `topic1/2/3` if needed.
    let filter = Filter::new()
        .address(address)
        .event_signature(BlockCommit::SIGNATURE_HASH)
        .from_block(from)
        .to_block(to);

    // Fetch all matching logs.
    let logs = provider
        .get_logs(&filter)
        .await
        .context("get_logs(BlockCommit)")?;

    let mut results = HashMap::new();

    // Batch transaction fetching: process logs in chunks of tx_batch_size
    for chunk in logs.chunks(tx_batch_size) {
        // Collect all tx hashes in this batch
        let tx_hashes: Vec<B256> = chunk
            .iter()
            .filter_map(|lg| lg.transaction_hash)
            .collect();

        // Fetch all transactions concurrently
        let tx_futures: Vec<_> = tx_hashes
            .iter()
            .map(|&hash| provider.get_transaction_by_hash(hash))
            .collect();

        let txs = futures::future::join_all(tx_futures).await;

        // Process results
        for (lg, tx_result) in chunk.iter().zip(txs.iter()) {
            let tx = match tx_result {
                Ok(Some(tx)) => tx,
                Ok(None) => {
                    tracing::warn!("Transaction not found for log {:?}", lg.transaction_hash);
                    continue;
                }
                Err(e) => {
                    tracing::error!("Failed to fetch transaction: {}", e);
                    continue;
                }
            };

            // Decode the event (topics+data) using the generated type.
            // Convert the RPC log into the primitives Log expected by the SolEvent decoder.
            let prim_log = alloy::primitives::Log {
                address: lg.address(),
                data: lg.data().clone(),
            };
            if let Ok(ev) = BlockCommit::decode_log(&prim_log) {
                let batch_info = BatchInfo::parse(ev, tx.clone());
                results.insert(batch_info.batch_number, batch_info);
            }
        }
    }

    Ok(results)
}

pub fn write_to_db(args: WriteToDBArgs) -> Result<()> {
    let json = std::fs::read_to_string(&args.input)
        .with_context(|| format!("read from {}", args.input))?;
    let blockchain_state: BlockchainState =
        serde_json::from_str(&json).context("parse blockchain state from JSON")?;

    tracing::info!(
        "Loaded blockchain state: {} batches, {} blocks, final tree root 0x{}",
        blockchain_state.current_batch,
        blockchain_state.current_block,
        hex::encode(blockchain_state.tree.compute_root())
    );

    sequencer_db::write_to_db(&args.db_path, blockchain_state)?;

    tracing::info!("Wrote blockchain state to RocksDB at {}", args.db_path);

    Ok(())
}
