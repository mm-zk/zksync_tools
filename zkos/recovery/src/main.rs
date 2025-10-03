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
    statediffs::{BatchInfo, BlockInfo},
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
    #[arg(long)]
    address: String,

    /// Start block (inclusive). If omitted, uses earliest.
    #[arg(long)]
    from: Option<u64>,

    /// End block (inclusive). If omitted, uses latest.
    #[arg(long)]
    to: Option<u64>,

    /// Chunk size (number of blocks per request)
    #[arg(long, default_value_t = 2_000u64)]
    chunk: u64,

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

async fn run_recover(args: RecoverArgs) -> Result<()> {
    // Load genesis from file.
    let genesis = init_genesis();

    let provider = ProviderBuilder::new().connect_http(args.rpc.parse()?);

    let target = Address::from_str(&args.address)
        .with_context(|| format!("invalid address: {}", args.address))?;

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
    let genesis_local_info = get_genesis_upgrade(&provider, target, from, to, args.chunk).await?;

    // Now we can create initial blockchain state.
    let mut blockchain_state = BlockchainState::new(genesis.clone(), genesis_local_info);

    // Then start scanning for 'CommitBatches' call, and collecting the state.
    let mut start = from;
    let chunks_total = (to - from) / args.chunk + 1;
    tracing::debug!("Total chunks to scan: {}", chunks_total);
    let mut chunks_done = 0;
    while start <= to {
        chunks_done += 1;
        if chunks_done % 10 == 0 {
            tracing::debug!("Progress: {}/{} chunks done", chunks_done, chunks_total);
        }
        let end = (start + args.chunk - 1).min(to);
        let scan_results = get_commit_batches_from_range(&provider, target, start, end).await?;

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

    for lg in logs {
        // Each log belongs to a tx; pull its calldata using the tx hash.
        let tx_hash: B256 = lg.transaction_hash.context("log missing tx hash")?;
        let tx = provider
            .get_transaction_by_hash(tx_hash)
            .await
            .with_context(|| format!("get_tx {}", tx_hash))?;
        let Some(tx) = tx else { continue };

        // Decode the event (topics+data) using the generated type.
        // Convert the RPC log into the primitives Log expected by the SolEvent decoder.
        let prim_log = alloy::primitives::Log {
            address: lg.address(),
            data: lg.data().clone(),
        };
        if let Ok(ev) = BlockCommit::decode_log(&prim_log) {
            let batch_info = BatchInfo::parse(ev, tx);
            results.insert(batch_info.batch_number, batch_info);
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
