use std::{collections::HashMap, str::FromStr};

use alloy::{
    consensus::Transaction,
    primitives::{Address, B256, U256, address},
    providers::{Provider, ProviderBuilder},
    rpc::types::Filter,
    sol,
    sol_types::{SolCall, SolEvent}, // for ABI-safe decoding of the commit function
};
use anyhow::{Context, Result, bail};
use blake2::{Blake2s256, Digest};
use clap::Parser;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;
use zk_os_basic_system::system_implementation::flat_storage_model::AccountProperties;

use crate::{
    deploy::BytecodeAnalysisResults,
    genesis::{GenesisUpgradeLocalInfo, get_genesis_upgrade},
    state::{LocalTree, init_genesis, init_tree_genesis},
    statediffs::{StateDiff, ValueDiff},
};

pub mod deploy;
pub mod genesis;
pub mod state;
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
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();
    let args = Args::parse();

    let genesis = init_genesis();

    let mut tree = init_tree_genesis();

    let mut preimage_store = HashMap::from_iter(genesis.preimages.iter().cloned());

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

    let mut start = from;

    let mut full_results = HashMap::new();
    let chunks_total = (to - from) / args.chunk + 1;
    tracing::debug!("Total chunks to scan: {}", chunks_total);
    let mut chunks_done = 0;
    while start <= to {
        chunks_done += 1;
        if chunks_done % 10 == 0 {
            tracing::debug!("Progress: {}/{} chunks done", chunks_done, chunks_total);
        }
        let end = (start + args.chunk - 1).min(to);
        let scan_results = scan_commits_via_logs(&provider, target, start, end).await?;
        full_results.extend(scan_results);
        start = end.saturating_add(1);
    }
    tracing::debug!("Scanned {} batches", full_results.len());

    let genesis_local_info = get_genesis_upgrade(&provider, target, from, to, args.chunk).await?;

    // get sorted keys from full_results
    let mut batch_numbers: Vec<u64> = full_results.keys().cloned().collect();
    batch_numbers.sort_unstable();
    let mut last_256_block_hashes = [B256::default(); 256];
    last_256_block_hashes[255] = genesis.header.hash_slow();
    let mut block_number = 0u64;

    for batch_number in &batch_numbers {
        let info = full_results.get(&batch_number).unwrap();

        tracing::debug!("Batch {}: {:?}", batch_number, info.batch_hash);

        assert_eq!(1, info.commits.len());
        let commit = &info.commits[0];

        for block_info in &info.blocks_data {
            tracing::debug!(
                "  Block: 0x{} with {} state diffs and {} logs",
                hex::encode(block_info.block_hash.as_slice()),
                block_info.state_diffs.len(),
                block_info.logs.len()
            );
            apply_batch(
                &mut tree,
                &mut preimage_store,
                block_info,
                &genesis_local_info,
            );
            for i in 0..255 {
                last_256_block_hashes[i] = last_256_block_hashes[i + 1];
            }

            last_256_block_hashes[255] = block_info.block_hash;
            block_number += 1;
        }

        let tree_root = tree.compute_root();
        let leaf_count: u64 = tree.leaves.len() as u64;

        tracing::debug!("Tree root: 0x{}", hex::encode(tree_root));

        tracing::debug!(
            "Expected state commitment: 0x{}",
            hex::encode(commit.newStateCommitment.as_slice())
        );
        let mut hasher = Blake2s256::new();
        hasher.update(tree_root.as_slice());
        hasher.update(leaf_count.to_be_bytes());
        hasher.update(block_number.to_be_bytes());
        tracing::debug!("Block number used: {}", block_number);

        let mut blocks_hasher = Blake2s256::new();
        for h in last_256_block_hashes.iter() {
            blocks_hasher.update(h.as_slice());
        }
        let last_256_block_hashes_blake = blocks_hasher.finalize();
        hasher.update(last_256_block_hashes_blake);
        // TODO: shoudl this be first or last?
        hasher.update(commit.lastBlockTimestamp.to_be_bytes());
        tracing::debug!("Block timestamp used: {}", commit.lastBlockTimestamp);
        let state_commitment = B256::from_slice(&hasher.finalize());
        tracing::debug!(
            "Computed state commitment: 0x{}",
            hex::encode(state_commitment)
        );

        assert_eq!(
            commit.newStateCommitment, state_commitment,
            "State commitment mismatch"
        );
    }

    tracing::info!(
        "All {} batches and {} blocks applied successfully, final tree root: 0x{}",
        batch_numbers.len(),
        block_number,
        hex::encode(tree.compute_root())
    );

    Ok(())
}

pub struct BatchInfo {
    pub batch_number: u64,
    pub batch_hash: B256,
    pub commitment: B256,
    pub calldata: Vec<u8>,
    pub stored: StoredBatchInfo,
    pub commits: Vec<CommitBatchInfoZKsyncOS>,
    // Block hash, state diffs, logs
    pub blocks_data: Vec<BlockInfo>,
}

pub struct BlockInfo {
    pub block_hash: B256,
    pub state_diffs: Vec<StateDiff>,
    pub logs: Vec<statediffs::Log>,
}

async fn scan_commits_via_logs<P: Provider + Clone>(
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
            let batch = ev.batchNumber;
            let batch_hash = ev.batchHash;
            let commitment = ev.commitment;

            let calldata = tx.input().clone();

            tracing::debug!(
                "{{\"block\":{},\"tx\":\"0x{}\",\"batchNumber\":{},\"batchHash\":\"0x{}\",\"commitment\":\"0x{}\"}}",
                lg.block_number.unwrap_or_default(),
                hex::encode(tx_hash.as_slice()),
                batch,
                hex::encode(batch_hash.as_slice()),
                hex::encode(commitment.as_slice()),
                //hex::encode(&calldata),
            );

            let commit_call = commitBatchesSharedBridgeCall::abi_decode(&calldata).unwrap();

            let commit_data_without_prefix = &commit_call._commitData[1..]; // skip the first byte (version)

            let decode_params =
                __decodeParamsCall::abi_decode_raw(commit_data_without_prefix).unwrap();

            let (stored, commits) = (decode_params._0, decode_params._1);

            let tmp = &commits[0];
            assert_eq!(1, commits.len());

            let blocks_data = parse_da_input(&tmp.operatorDAInput).unwrap();

            let batch_number: u64 = batch.try_into().unwrap();
            results.insert(
                batch_number,
                BatchInfo {
                    batch_number,
                    batch_hash,
                    commitment,
                    calldata: calldata.to_vec(),
                    stored,
                    commits,
                    blocks_data,
                },
            );
        }
    }

    Ok(results)
}

#[derive(Debug)]
pub struct BytecodeInfo {
    pub hash: B256,
    pub len: U256,
    // not sure what that is..
    pub observable_hash: B256,

    pub hash_with_artifacts: B256,
    pub artifacts_len: usize,
    // we'll probably have to add bytecodes too.
}

impl BytecodeInfo {
    pub fn parse(
        bytecode_info: &[u8],
        factory_deps: &HashMap<B256, BytecodeAnalysisResults>,
    ) -> Self {
        if bytecode_info.len() != 96 {
            panic!("bytecode info wrong length: {}", bytecode_info.len());
        }
        let hash = B256::try_from(&bytecode_info[0..32]).unwrap();
        let len = U256::from_be_slice(&bytecode_info[32..64]);
        let observable_hash = B256::try_from(&bytecode_info[64..96]).unwrap();
        tracing::debug!("bytecode info hash: 0x{}", hex::encode(hash));
        tracing::debug!("bytecode info len: {}", len);
        tracing::debug!(
            "bytecode info observable hash: 0x{}",
            hex::encode(observable_hash)
        );
        let analysis_result = factory_deps.get(&hash).unwrap_or_else(|| {
            panic!(
                "bytecode info hash not found in factory deps: 0x{}",
                hex::encode(hash.as_slice())
            )
        });
        assert_eq!(hash, analysis_result.bytecode_hash);
        Self {
            hash,
            len,
            observable_hash,
            hash_with_artifacts: analysis_result.hash_with_artifacts,
            artifacts_len: analysis_result.artifacts_len,
        }
    }
}

pub fn parse_da_input(input: &[u8]) -> Result<Vec<BlockInfo>> {
    // first 32 bytes should be 0, the next 32 is some keccak.
    if input.len() < 64 {
        tracing::error!("DA input too short: {}", input.len());
        return Err(anyhow::anyhow!("DA input too short"));
    }
    // not sure what this prefix is..
    let prefix = &input[0..32];
    let pubdata_hash = &input[32..64];
    if prefix.iter().any(|&b| b != 0) {
        tracing::error!("DA input prefix not zero: {:x?}", prefix);
        return Err(anyhow::anyhow!("DA input prefix not zero"));
    }
    tracing::debug!("pubdata input hash: 0x{}", hex::encode(pubdata_hash));
    let blob_count = &input[64];
    // for calldata, blobcount should be 1.

    tracing::debug!("blob count: {}", blob_count);
    let mut offset = 65;
    // another 32 bytes that should be 0.
    if input.len() < offset + 32 {
        tracing::error!("DA input too short for second zero: {}", input.len());
        return Err(anyhow::anyhow!("DA input too short for second zero"));
    }
    let mid = &input[offset..offset + 32];
    if mid.iter().any(|&b| b != 0) {
        tracing::error!("DA input mid not zero: {:x?}", mid);
        return Err(anyhow::anyhow!("DA input mid not zero"));
    }
    offset += 32;

    let calldata_type = &input[offset];
    tracing::debug!("calldata type: {}", calldata_type);
    assert_eq!(&0, calldata_type); // we only handle calldata type 0

    offset += 1;

    // remaining bytes:

    let mut results = vec![];
    loop {
        tracing::debug!("parsing block {}", results.len());
        let (consumed, block_header_hash, state_diff, logs) = parse_block_da(&input[offset..])?;
        tracing::debug!(
            "offset: {} consumed: {} input len: {}",
            offset,
            consumed,
            input.len()
        );
        offset += consumed;
        results.push(BlockInfo {
            block_hash: block_header_hash,
            state_diffs: state_diff,
            logs,
        });
        // TODO: seems that last 32 bytes are 0s..
        if offset + 32 >= input.len() {
            break;
        }
    }
    let blob_commitment = B256::from_slice(&input[offset..offset + 32]);
    tracing::debug!("blob commitment: {:?}", blob_commitment);
    assert_eq!(blob_commitment, B256::ZERO);

    Ok(results)
}

pub fn parse_block_da(input: &[u8]) -> Result<(usize, B256, Vec<StateDiff>, Vec<statediffs::Log>)> {
    let pubdata = input;
    tracing::debug!("pubdata len: {}", pubdata.len());

    // now for pubdata itself.

    // First 32 should be some hash.
    if pubdata.len() < 32 {
        tracing::error!("pubdata too short for hash: {}", pubdata.len());
        return Err(anyhow::anyhow!("pubdata too short for hash"));
    }
    // This is the 'current_block_hash' from io_subsystem.rs 'finish'
    let block_header_hash = B256::from_slice(&pubdata[0..32]);
    tracing::debug!("block header hash: 0x{}", hex::encode(block_header_hash));

    let (state_diff_offset, state_diff) = StateDiff::new_from_stream(&pubdata[32..]);

    let remaining = &pubdata[32 + state_diff_offset as usize..];

    // u32 for logs length
    if remaining.len() < 4 {
        tracing::error!("pubdata too short for logs len: {}", remaining.len());
        return Err(anyhow::anyhow!("pubdata too short for logs len"));
    }
    let logs_len = u32::from_be_bytes(
        remaining[0..4]
            .try_into()
            .expect("slice with incorrect length"),
    );
    tracing::debug!("pubdata logs len: {}", logs_len);

    let mut offset = 4;

    let mut logs = Vec::new();

    for _ in 0..logs_len {
        let (consumed, log) = statediffs::Log::new_from_stream(&remaining[offset..]);

        logs.push(log);
        offset += consumed as usize;
    }

    let messages_len: u32 = if remaining.len() < offset + 4 {
        tracing::error!("pubdata too short for messages len: {}", remaining.len());
        return Err(anyhow::anyhow!("pubdata too short for messages len"));
    } else {
        u32::from_be_bytes(
            remaining[offset..offset + 4]
                .try_into()
                .expect("slice with incorrect length"),
        )
    };
    offset += 4;

    tracing::debug!("pubdata messages len: {}", messages_len);

    if messages_len > 0 {
        for _ in 0..messages_len {
            let len = u32::from_be_bytes(
                remaining[offset..offset + 4]
                    .try_into()
                    .expect("slice with incorrect length"),
            );
            offset += 4;
            tracing::trace!("message len: {}", len);
            offset += len as usize;
        }
    }

    tracing::debug!("pubdata remaining len: {}", remaining.len() - offset);

    Ok((
        offset + 32 + state_diff_offset as usize,
        block_header_hash,
        state_diff,
        logs,
    ))
}

pub fn address_to_b256(address: &Address) -> B256 {
    let mut extended_address = [0u8; 32];
    extended_address[12..].copy_from_slice(&address.0.0);
    B256::from(extended_address)
}

pub fn derive_properties_storage_address(address: &Address) -> B256 {
    let account_properties_address = address!("0000000000000000000000000000000000008003");

    let mut hasher = Blake2s256::new();
    hasher.update(address_to_b256(&account_properties_address));
    hasher.update(address_to_b256(address));

    let hash = hasher.finalize();
    B256::from_slice(&hash)
}

pub fn apply_batch(
    tree: &mut LocalTree,
    preimage_store: &mut HashMap<B256, Vec<u8>>,
    info: &BlockInfo,
    genesis_info: &GenesisUpgradeLocalInfo, // In future, this should also cover upgraded and l1 tx.
) {
    let mut force_deploy_map = HashMap::new();
    // Change force deploy info into derived key.
    for (addr, bytecode_info) in &genesis_info.force_deploy_info {
        let derived_key = derive_properties_storage_address(addr);

        tracing::trace!(
            "Applying force-deploy for addr 0x{} at key 0x{:x}",
            hex::encode(addr.as_slice()),
            derived_key
        );
        force_deploy_map.insert(derived_key, bytecode_info);
    }

    for diff in &info.state_diffs {
        match diff.value {
            statediffs::StateDiffValue::AccountProperties(ref ap) => {
                tracing::debug!(
                    "Applying AccountProperties diff for key 0x{:x}",
                    diff.derived_key
                );
                tracing::trace!("**AccountProperties diff: {:#?}", ap);
                let account_hash = tree.get_value(diff.derived_key);
                tracing::debug!("**account hash: {:#x}", account_hash);

                let properties = if account_hash.is_zero() {
                    AccountProperties::default()
                } else {
                    AccountProperties::decode(
                        &preimage_store
                            .get(&account_hash)
                            .unwrap()
                            .clone()
                            .try_into()
                            .unwrap(),
                    )
                };
                let properties =
                    ap.update_itself(properties, force_deploy_map.get(&diff.derived_key));

                tracing::trace!("**new account properties: {:#?}", properties);
                let properties_hash = properties.compute_hash();
                preimage_store.insert(
                    properties_hash.as_u8_array().into(),
                    properties.encoding().to_vec(),
                );
                tree.add_entry(diff.derived_key, properties_hash.as_u8_array().into());
            }
            statediffs::StateDiffValue::Value(ref v) => {
                apply_value_diff(tree, diff.derived_key, v);
            }
        }
    }
}

pub fn u256_to_b256(value: &U256) -> B256 {
    let bytes = value.to_be_bytes();

    B256::from(bytes)
}

pub fn b256_to_u256(value: &B256) -> U256 {
    U256::from_be_bytes(value.0)
}

pub fn apply_value_diff(tree: &mut LocalTree, key: B256, diff: &ValueDiff) {
    match diff {
        ValueDiff::Nothing(v) => {
            tree.add_entry(key, u256_to_b256(v));
        }
        ValueDiff::Add(v) => tree.add_entry(
            key,
            u256_to_b256(&b256_to_u256(&tree.get_value(key)).wrapping_add(*v)),
        ),
        ValueDiff::Sub(v) => {
            tree.add_entry(
                key,
                u256_to_b256(&b256_to_u256(&tree.get_value(key)).wrapping_sub(*v)),
            );
        }
        ValueDiff::Transform(v) => {
            tree.add_entry(key, u256_to_b256(v));
        }
    };
}
