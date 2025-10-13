use std::{collections::HashMap, str::FromStr};

use alloy::{
    primitives::Address,
    providers::{Provider, ProviderBuilder},
    rpc::types::Filter,
    sol_types::SolEvent, // for ABI-safe decoding of the commit function
};
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

use crate::{
    chain_genesis::get_genesis_upgrade, contracts::BlockCommit, state::BlockchainState,
    state_genesis::init_genesis, statediffs::BatchInfo,
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

    /// WriteBatch chunk size (number of entries per batch)
    #[arg(long, default_value_t = 100_000)]
    batch_size: usize,
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

    /// Concurrency (number of concurrent operations for both chunk scanning and transaction fetching).
    #[arg(long, default_value_t = 10)]
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
        Address::from_str(&address).with_context(|| format!("invalid address: {}", address))?
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
        bail!(
            "Either --address must be provided, or both --bridgehub and --chain-id for auto-discovery"
        );
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
    let genesis_local_info =
        get_genesis_upgrade(&provider, target, from, to, args.chunk, args.concurrency).await?;

    // Now we can create initial blockchain state.
    let mut blockchain_state = BlockchainState::new(genesis.clone(), genesis_local_info);

    // Phase 1: Scan for CommitBatches events
    let all_logs_with_hashes =
        scan_commit_events(&provider, target, from, to, args.chunk, args.concurrency).await?;

    // Phase 2: Fetch transactions and decode batches
    let all_batches =
        fetch_and_decode_batches(&provider, all_logs_with_hashes, args.concurrency).await?;

    // Apply batches in order after all chunks are collected
    let mut batch_numbers = all_batches.keys().cloned().collect::<Vec<_>>();
    batch_numbers.sort_unstable();

    tracing::info!("Applying {} batches in order...", batch_numbers.len());
    for batch_number in batch_numbers {
        let batch_info = all_batches.get(&batch_number).unwrap();
        blockchain_state.apply_batch(batch_number, batch_info);
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

/// Phase 1: Scan all chunks for CommitBatches events, collecting transaction hashes
async fn scan_commit_events<P: Provider + Clone>(
    provider: &P,
    target: Address,
    from: u64,
    to: u64,
    chunk_size: u64,
    concurrency: usize,
) -> Result<Vec<(alloy::primitives::B256, alloy::rpc::types::Log)>> {
    use futures::stream::{self, StreamExt};

    let chunks_total = (to - from) / chunk_size + 1;
    tracing::info!(
        "Scanning for CommitBatches events: {} total chunks (concurrency={})",
        chunks_total,
        concurrency
    );

    let chunk_ranges: Vec<_> = (0..chunks_total)
        .map(|i| {
            let start = from + (i * chunk_size);
            let end = (start + chunk_size - 1).min(to);
            (start, end)
        })
        .collect();

    let mut chunk_stream = stream::iter(chunk_ranges)
        .map(|(start, end)| {
            let provider = provider.clone();
            async move { get_commit_batches_from_range(&provider, target, start, end).await }
        })
        .buffer_unordered(concurrency);

    let mut chunks_done = 0;
    let mut all_logs_with_hashes = Vec::new();
    let mut last_log_time = std::time::Instant::now();

    while let Some(result) = chunk_stream.next().await {
        chunks_done += 1;
        all_logs_with_hashes.extend(result?);

        let should_log = chunks_done % concurrency == 0 || last_log_time.elapsed().as_secs() >= 5;
        if should_log {
            tracing::info!(
                "Commit scan progress: {}/{} chunks ({:.1}%), found {} commit events so far",
                chunks_done,
                chunks_total,
                (chunks_done as f64 / chunks_total as f64) * 100.0,
                all_logs_with_hashes.len()
            );
            last_log_time = std::time::Instant::now();
        }
    }

    tracing::info!(
        "Finished scanning all chunks, found {} commit events total",
        all_logs_with_hashes.len()
    );

    Ok(all_logs_with_hashes)
}

/// Phase 2: Fetch all transactions and decode into BatchInfo
async fn fetch_and_decode_batches<P: Provider + Clone>(
    provider: &P,
    logs_with_hashes: Vec<(alloy::primitives::B256, alloy::rpc::types::Log)>,
    concurrency: usize,
) -> Result<HashMap<u64, BatchInfo>> {
    use futures::stream::{self, StreamExt};

    let total = logs_with_hashes.len();
    tracing::info!(
        "Fetching {} transactions with concurrency {}...",
        total,
        concurrency
    );

    let fetch_start = std::time::Instant::now();
    let mut last_log_time = std::time::Instant::now();

    let mut tx_stream = stream::iter(logs_with_hashes)
        .map(|(tx_hash, lg)| {
            let provider = provider.clone();
            async move {
                let tx_result = provider.get_transaction_by_hash(tx_hash).await;
                (tx_hash, lg, tx_result)
            }
        })
        .buffer_unordered(concurrency);

    let mut all_batches = HashMap::new();
    let mut processed = 0;

    while let Some((tx_hash, lg, tx_result)) = tx_stream.next().await {
        processed += 1;

        if last_log_time.elapsed().as_secs() >= 5 {
            tracing::info!(
                "Transaction fetch progress: {}/{} ({:.1}%)",
                processed,
                total,
                (processed as f64 / total as f64) * 100.0
            );
            last_log_time = std::time::Instant::now();
        }

        let tx = match tx_result {
            Ok(Some(tx)) => tx,
            Ok(None) => {
                tracing::warn!("Transaction not found for hash {:?}", tx_hash);
                continue;
            }
            Err(e) => {
                tracing::error!("Failed to fetch transaction {:?}: {}", tx_hash, e);
                continue;
            }
        };

        let prim_log = alloy::primitives::Log {
            address: lg.address(),
            data: lg.data().clone(),
        };

        if let Ok(ev) = BlockCommit::decode_log(&prim_log) {
            let batch_info = BatchInfo::parse(ev, tx);
            all_batches.insert(batch_info.batch_number, batch_info);
        }
    }

    tracing::info!(
        "Finished fetching all transactions in {:.2}s ({:.0} tx/s), collected {} batches total",
        fetch_start.elapsed().as_secs_f64(),
        total as f64 / fetch_start.elapsed().as_secs_f64(),
        all_batches.len()
    );

    Ok(all_batches)
}

async fn get_commit_batches_from_range<P: Provider + Clone>(
    provider: &P,
    address: Address,
    from: u64,
    to: u64,
) -> Result<Vec<(alloy::primitives::B256, alloy::rpc::types::Log)>> {
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

    // Collect logs with their transaction hashes
    let results: Vec<_> = logs
        .into_iter()
        .filter_map(|log| log.transaction_hash.map(|hash| (hash, log)))
        .collect();

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

    sequencer_db::write_to_db(&args.db_path, blockchain_state, args.batch_size)?;

    tracing::info!("Wrote blockchain state to RocksDB at {}", args.db_path);

    Ok(())
}
