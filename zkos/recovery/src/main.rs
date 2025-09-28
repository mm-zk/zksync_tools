use std::str::FromStr;

use alloy::{
    consensus::Transaction,
    primitives::{Address, B256},
    providers::{Provider, ProviderBuilder},
    rpc::types::Filter,
    sol,
    sol_types::SolEvent, // for ABI-safe decoding of the commit function
};
use anyhow::{Context, Result, bail};
use clap::Parser;

// Define the function we care about for optional decoding.
// This generates `commitBatchesSharedBridgeCall` rust type with `abi_decode`.
sol! {
    function commitBatchesSharedBridge(
        address _chainAddress,
        uint256 _processFrom,
        uint256 _processTo,
        bytes _commitData
    );
    event BlockCommit(uint256 indexed batchNumber, bytes32 indexed batchHash, bytes32 indexed commitment);


}

#[derive(Debug, Parser)]
#[command(
    name = "l1-commit-scraper",
    about = "Scan Ethereum L1 and extract calldata for calls to a contract"
)]
struct Args {
    /// Ethereum RPC URL (archive preferred)
    #[arg(long)]
    rpc: String,

    /// Target contract address (0x...)
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

    /// If set, attempt to decode inputs as commitBatchesSharedBridge
    #[arg(long, default_value_t = true)]
    decode_commit: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

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

    eprintln!(
        "Scanning {} -> {} ({} blocks) on {}",
        from,
        to,
        to - from + 1,
        args.rpc
    );

    let mut start = from;
    while start <= to {
        let end = (start + args.chunk - 1).min(to);
        //scan_range(&provider, target, start, end, args.decode_commit).await?;
        scan_commits_via_logs(&provider, target, start, end).await?;
        start = end.saturating_add(1);
    }

    Ok(())
}

async fn scan_commits_via_logs<P: Provider + Clone>(
    provider: &P,
    address: Address,
    from: u64,
    to: u64,
) -> Result<()> {
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
            data: lg.data().clone(), /*data: alloy::primitives::LogData:::new_unchecked(
                                         lg.topics().clone().into(),
                                         lg.data().clone(),
                                     ),*/
        };
        if let Ok(ev) = BlockCommit::decode_log(&prim_log) {
            let batch = ev.batchNumber;
            let batch_hash = ev.batchHash;
            let commitment = ev.commitment;

            let calldata = tx.input().clone();

            println!(
                "{{\"block\":{},\"tx\":\"0x{}\",\"batchNumber\":{},\"batchHash\":\"0x{}\",\"commitment\":\"0x{}\",\"calldata\":\"0x{}\"}}",
                lg.block_number.unwrap_or_default(),
                hex::encode(tx_hash.as_slice()),
                batch,
                hex::encode(batch_hash.as_slice()),
                hex::encode(commitment.as_slice()),
                hex::encode(calldata),
            );
        }
    }

    Ok(())
}
