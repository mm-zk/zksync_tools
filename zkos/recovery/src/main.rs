use std::{collections::HashMap, str::FromStr};

use alloy::{
    primitives::{Address, B256},
    providers::{Provider, ProviderBuilder},
    rpc::types::Filter,
    sol,
    sol_types::SolEvent, // for ABI-safe decoding of the commit function
};
use anyhow::{Context, Result, bail};
use clap::Parser;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

use crate::{
    chain_genesis::get_genesis_upgrade,
    state::BlockchainState,
    state_genesis::init_genesis,
    statediffs::{BatchInfo, BlockInfo},
};

pub mod bytecodes;
pub mod chain_genesis;
pub mod state;
pub mod state_genesis;
pub mod statediffs;

// Define the function we care about for optional decoding.
// This generates `commitBatchesSharedBridgeCall` rust type with `abi_decode`.
sol! {
    #[derive(Debug)]
    function commitBatchesSharedBridge(
        address _chainAddress,
        uint256 _processFrom,
        uint256 _processTo,
        bytes _commitData
    );
    event BlockCommit(uint256 indexed batchNumber, bytes32 indexed batchHash, bytes32 indexed commitment);


    struct L2CanonicalTransaction {
        uint256 txType;
        uint256 from;
        uint256 to;
        uint256 gasLimit;
        uint256 gasPerPubdataByteLimit;
        uint256 maxFeePerGas;
        uint256 maxPriorityFeePerGas;
        uint256 paymaster;
        uint256 nonce;
        uint256 value;
        // In the future, we might want to add some
        // new fields to the struct. The `txData` struct
        // is to be passed to account and any changes to its structure
        // would mean a breaking change to these accounts. To prevent this,
        // we should keep some fields as "reserved"
        // It is also recommended that their length is fixed, since
        // it would allow easier proof integration (in case we will need
        // some special circuit for preprocessing transactions)
        uint256[4] reserved;
        bytes data;
        bytes signature;
        uint256[] factoryDeps;
        bytes paymasterInput;
        // Reserved dynamic type for the future use-case. Using it should be avoided,
        // But it is still here, just in case we want to enable some additional functionality
        bytes reservedDynamic;
    }

    event NewPriorityRequest(
        uint256 txId,
        bytes32 txHash,
        uint64 expirationTimestamp,
        L2CanonicalTransaction transaction,
        bytes[] factoryDeps
    );

    event GenesisUpgrade(
        address indexed _zkChain,
        L2CanonicalTransaction _l2Transaction,
        uint256 indexed _protocolVersion,
        bytes[] _factoryDeps
    );

    #[derive(Debug)]
    struct StoredBatchInfo {
        uint64 batchNumber;
        bytes32 batchHash;
        uint64 indexRepeatedStorageChanges;
        uint256 numberOfLayer1Txs;
        bytes32 priorityOperationsHash;
        bytes32 dependencyRootsRollingHash;
        bytes32 l2LogsTreeRoot;
        uint256 timestamp;
        bytes32 commitment;
    }

    #[derive(Debug)]
    struct CommitBatchInfoZKsyncOS {
        uint64 batchNumber;
        bytes32 newStateCommitment;
        uint256 numberOfLayer1Txs;
        bytes32 priorityOperationsHash;
        bytes32 dependencyRootsRollingHash;
        bytes32 l2LogsTreeRoot;
        address l2DaValidator;
        bytes32 daCommitment;
        uint64 firstBlockTimestamp;
        uint64 lastBlockTimestamp;
        uint256 chainId;
        bytes operatorDAInput;
    }

    // A dummy function that takes the same parameters we encoded.
    function __decodeParams(StoredBatchInfo, CommitBatchInfoZKsyncOS[]);


    // Stuff needed for genesis upgrade tx decoding
    function upgrade(address delegateTo, bytes _calldata);
    function genesisUpgrade(
        bool _isZKsyncOS,
        uint256 _chainId,
        address _ctmDeployer,
        bytes calldata _fixedForceDeploymentsData,
        bytes calldata _additionalForceDeploymentsData
    );

    #[derive(Debug)]
    struct FixedForceDeploymentsData {
        uint256 l1ChainId;
        uint256 eraChainId;
        address l1AssetRouter;
        bytes32 l2TokenProxyBytecodeHash;
        address aliasedL1Governance;
        uint256 maxNumberOfZKChains;
        bytes bridgehubBytecodeInfo;
        bytes l2AssetRouterBytecodeInfo;
        bytes l2NtvBytecodeInfo;
        bytes messageRootBytecodeInfo;
        bytes chainAssetHandlerBytecodeInfo;
        bytes beaconDeployerInfo;
        address l2SharedBridgeLegacyImpl;
        address l2BridgedStandardERC20Impl;
        // The forced beacon address. It is needed only for internal testing.
        // MUST be equal to 0 in production.
        // It will be the job of the governance to ensure that this value is set correctly.
        address dangerousTestOnlyForcedBeacon;
    }


    #[derive(Debug)]
    struct ZKChainSpecificForceDeploymentsData {
        bytes32 baseTokenAssetId;
        address l2LegacySharedBridge;
        address predeployedL2WethAddress;
        address baseTokenL1Address;
        /// @dev Some info about the base token, it is
        /// needed to deploy weth token in case it is not present
        string baseTokenName;
        string baseTokenSymbol;
    }



}

#[derive(Debug, Parser)]
#[command(
    name = "l1-recovery",
    about = "Scan Ethereum L1 and recover zkSync chain state"
)]
struct Args {
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
    let args = Args::parse();

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
