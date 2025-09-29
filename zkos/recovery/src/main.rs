use std::{collections::HashMap, str::FromStr};

use alloy::{
    consensus::Transaction,
    dyn_abi::SolType,
    primitives::{Address, B256, U256},
    providers::{Provider, ProviderBuilder},
    rpc::types::Filter,
    sol,
    sol_types::{SolCall, SolEvent}, // for ABI-safe decoding of the commit function
};
use anyhow::{Context, Result, bail};
use clap::Parser;
use zk_os_basic_system::system_implementation::flat_storage_model::AccountProperties;

use crate::{
    state::{LocalTree, init_genesis, init_tree_genesis},
    statediffs::{StateDiff, ValueDiff},
};

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

    eprintln!(
        "Scanning {} -> {} ({} blocks) on {}",
        from,
        to,
        to - from + 1,
        args.rpc
    );

    let mut start = from;

    let mut full_results = HashMap::new();
    while start <= to {
        let end = (start + args.chunk - 1).min(to);
        //scan_range(&provider, target, start, end, args.decode_commit).await?;
        let scan_results = scan_commits_via_logs(&provider, target, start, end).await?;
        full_results.extend(scan_results);
        start = end.saturating_add(1);
    }
    eprintln!("Scanned {} batches", full_results.len());

    // get sorted keys from full_results
    let mut batch_numbers: Vec<u64> = full_results.keys().cloned().collect();
    batch_numbers.sort_unstable();
    for batch_number in batch_numbers {
        let info = full_results.get(&batch_number).unwrap();

        println!("Batch {}: {:?}", batch_number, info.batch_hash);

        apply_batch(&mut tree, &mut preimage_store, info);
    }

    Ok(())
}

pub struct BatchInfo {
    pub batch_number: u64,
    pub batch_hash: B256,
    pub commitment: B256,
    pub calldata: Vec<u8>,
    pub stored: StoredBatchInfo,
    pub commits: Vec<CommitBatchInfoZKsyncOS>,
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

            let (state_diffs, logs) = parse_da_input(&tmp.operatorDAInput).unwrap();

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
                    state_diffs,
                    logs,
                },
            );
        }
    }

    Ok(results)
}

pub fn parse_da_input(input: &[u8]) -> Result<(Vec<StateDiff>, Vec<statediffs::Log>)> {
    // first 32 bytes should be 0, the next 32 is some keccak.
    if input.len() < 64 {
        eprintln!("DA input too short: {}", input.len());
        return Err(anyhow::anyhow!("DA input too short"));
    }
    // not sure what this prefix is..
    let prefix = &input[0..32];
    let pubdata_hash = &input[32..64];
    if prefix.iter().any(|&b| b != 0) {
        eprintln!("DA input prefix not zero: {:x?}", prefix);
        return Err(anyhow::anyhow!("DA input prefix not zero"));
    }
    eprintln!("pubdata input hash: 0x{}", hex::encode(pubdata_hash));
    let blob_count = &input[64];
    // for calldata, blobcount should be 1.

    eprintln!("blob count: {}", blob_count);
    let mut offset = 65;
    // another 32 bytes that should be 0.
    if input.len() < offset + 32 {
        eprintln!("DA input too short for second zero: {}", input.len());
        return Err(anyhow::anyhow!("DA input too short for second zero"));
    }
    let mid = &input[offset..offset + 32];
    if mid.iter().any(|&b| b != 0) {
        eprintln!("DA input mid not zero: {:x?}", mid);
        return Err(anyhow::anyhow!("DA input mid not zero"));
    }
    offset += 32;

    let calldata_type = &input[offset];
    eprintln!("calldata type: {}", calldata_type);
    assert_eq!(&0, calldata_type); // we only handle calldata type 0

    offset += 1;

    // remaining bytes:
    let pubdata = &input[offset..];
    eprintln!("pubdata len: {}", pubdata.len());

    // now for pubdata itself.

    // First 32 should be some hash.
    if pubdata.len() < 32 {
        eprintln!("pubdata too short for hash: {}", pubdata.len());
        return Err(anyhow::anyhow!("pubdata too short for hash"));
    }
    // This is the 'current_block_hash' from io_subsystem.rs 'finish'
    let pubdata_hash2 = &pubdata[0..32];
    eprintln!("pubdata?? hash: 0x{}", hex::encode(pubdata_hash2));

    let (state_diff_offset, state_diff) = StateDiff::new_from_stream(&pubdata[32..]);
    //eprintln!("pubdata parsed len: {}", state_diff_offset);
    //eprintln!("pubdata state diff: {:#?}", state_diff);

    let remaining = &pubdata[32 + state_diff_offset as usize..];

    // u32 for logs length
    if remaining.len() < 4 {
        eprintln!("pubdata too short for logs len: {}", remaining.len());
        return Err(anyhow::anyhow!("pubdata too short for logs len"));
    }
    let logs_len = u32::from_be_bytes(
        remaining[0..4]
            .try_into()
            .expect("slice with incorrect length"),
    );
    eprintln!("pubdata logs len: {}", logs_len);

    let mut offset = 4;

    let mut logs = Vec::new();

    for _ in 0..logs_len {
        let (consumed, log) = statediffs::Log::new_from_stream(&remaining[offset..]);
        //println!("log: {:#?}", log);

        logs.push(log);
        offset += consumed as usize;
    }

    let messages_len: u32 = if remaining.len() < offset + 4 {
        eprintln!("pubdata too short for messages len: {}", remaining.len());
        return Err(anyhow::anyhow!("pubdata too short for messages len"));
    } else {
        u32::from_be_bytes(
            remaining[offset..offset + 4]
                .try_into()
                .expect("slice with incorrect length"),
        )
    };
    offset += 4;

    println!("pubdata messages len: {}", messages_len);

    if messages_len > 0 {
        todo!();
    }

    println!("pubdata remaining len: {}", remaining.len() - offset);
    assert_eq!(remaining.len() - offset, 32);

    let last_slot = B256::from_slice(&remaining[offset..offset + 32]);
    println!("last slot: {:?}", last_slot);
    assert_eq!(last_slot, B256::ZERO);

    Ok((state_diff, logs))
}

/*

// some 'zero' ?
0000000000000000000000000000000000000000000000000000000000000000
// keccak of pubdata
6eb0d00bd36db7ddad60a3cd5b94a289466d825c2038ff8393f451d634c9bd63
01 // 1 ??
// another 0 ?
0000000000000000000000000000000000000000000000000000000000000000
// calldata
00
// pubdata - concat from many blocks (but we have only 1)
//  -- maybe some hash?
b76ffe1f37a1892d5fbf5284c611035bf1616584838ce7772481030027cd950d


00000002003ac1e7247f50b6ea3ed2d1c63ce2511668e0d06882fa7a44bbcb0fb31c2e2e1c09010a6431f21254a08b1b0060d9dc74ad5f1ff038e24a6ebc26260a4a03a8d036011b591409640000000000000000


// some finishing 0s.
0000000000000000000000000000000000000000000000000000000000000000


on 'finish' we push current block hash.

*/

/* decoding experiment


00 -- ?
000002003ac1e7247f50b6ea3ed2d1c63ce2511668e0d06882fa7a44bbcb0fb3

1c
2e2e1c09010a6431f21254a08b1b0060d9dc74ad5f1ff038e24a6ebc26260a4a03a8d036011b591409640000000000000000

// experiment 2:
( I've transferred 100 (0x64) wei, so it has to be somewhere.)


// Start with fiat_storage_model
// first u32 -- is number of diffs.


00000002 -- ok, 2 diffs.

// Then for each key - it will be eitehr a storage slot or entry in account properties.
// But first 32 will always be the address.

003ac1e7247f50b6ea3ed2d1c63ce2511668e0d06882fa7a44bbcb0fb31c2e2e

1c -- 28 -- this means it is a 'small' diff, where both nonce and balance hash changed.
// now comes the 'nonce compression'

09 -- (this is 'add' - lenght == 1, )

01 --nonce increased by 1


0a -- this is a 'sub' with length = 1

64 -- and we subtracted 100 (0x64) from balance.

// now comes second diff key
31f21254a08b1b0060d9dc74ad5f1ff038e24a6ebc26260a4a03a8d036011b59


14 -- so minimal plus only balance changed

09 -- so this is 'add'

64 -- 0x64 - 100 - added.



00000000 -- these could be u32 for logs
00000000 -- and this is for messages.

*/

pub fn apply_batch(
    tree: &mut LocalTree,
    preimage_store: &mut HashMap<B256, Vec<u8>>,
    info: &BatchInfo,
) {
    for diff in &info.state_diffs {
        match diff.value {
            statediffs::StateDiffValue::AccountProperties(ref ap) => {
                println!("**AccountProperties diff: {:#?}", ap);
                let account_hash = tree.get_value(diff.derived_key);
                println!("**account hash: {:#x}", account_hash);

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
                let properties = ap.update_itself(properties);

                println!("**new account properties: {:#?}", properties);
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
