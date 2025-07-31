use std::collections::{HashMap, HashSet};

use alloy::{
    primitives::{Address, B256, U256, keccak256},
    signers::local::PrivateKeySigner,
};
use clap::{Parser, Subcommand};

use alloy::{
    consensus::Transaction,
    hex::FromHex,
    providers::{Provider, ProviderBuilder},
    rpc::types::Filter,
    sol,
    sol_types::{SolCall, SolEvent},
};

use crate::{
    execute::execute_batches,
    prove::{fake_prove_batches, prove_batches},
};

mod execute;
mod l1_merkle;
mod prove;
mod snark;

sol! {
    #[sol(rpc)]
    contract IHyperchain {
        function getVerifier() external view returns (address);
        function getAdmin() external view returns (address);
        function getTotalBatchesCommitted() external view returns (uint256);
        function getTotalBatchesVerified() external view returns (uint256);
        function getTotalBatchesExecuted() external view returns (uint256);
        function getSemverProtocolVersion() external view returns (uint32, uint32, uint32);

        function getL2BootloaderBytecodeHash() external view returns (bytes32);
        function getL2DefaultAccountBytecodeHash() external view returns (bytes32);
        function getL2SystemContractsUpgradeTxHash() external view returns (bytes32);
        function getChainId() external view returns (uint256);
        function getSettlementLayer() external view returns (address);

        function getPriorityQueueSize() external view returns (uint256);
        function getTotalPriorityTxs() external view returns (uint256);
        function getPriorityTreeRoot() external view returns (bytes32);

        function commitBatchesSharedBridge(uint256,uint256,uint256,bytes);

        function tmpStuff(StoredBatchInfo stored, CommitBoojumOSBatchInfo[] commits) external;

        function proofPayload(StoredBatchInfo old, StoredBatchInfo[] newInfo, uint256[] proof);

        struct PriorityOpsBatchInfo {
            bytes32[] leftPath;
            bytes32[] rightPath;
            bytes32[] itemHashes;
        }

        function executePayload(StoredBatchInfo[] executeData, PriorityOpsBatchInfo[] priorityOps);


        function proveBatchesSharedBridge(
            uint256, // _chainId
            uint256 _processBatchFrom,
            uint256 _processBatchTo,
            bytes calldata _proofData
        );

        function executeBatchesSharedBridge(
            uint256, // _chainId
            uint256 _processFrom,
            uint256 _processTo,
            bytes calldata _executeData
        );

        event BlockCommit(uint256 indexed batchNumber, bytes32 indexed batchHash, bytes32 indexed commitment);

    }
    #[derive(Debug)]

    struct StoredBatchInfo {
        uint64 batchNumber;
        bytes32 batchHash; // For Boojum OS batches we'll store here full state commitment
        uint64 indexRepeatedStorageChanges; // For Boojum OS not used, 0
        uint256 numberOfLayer1Txs;
        bytes32 priorityOperationsHash;
        bytes32 l2LogsTreeRoot;
        uint256 timestamp; // For Boojum OS not used, 0
        bytes32 commitment;// For Boojum OS batches we'll store batch output hash here
    }
    #[derive(Debug)]

    struct CommitBoojumOSBatchInfo {
        uint64 batchNumber;
        // chain state commitment, this preimage is not opened on l1,
        // it's guaranteed that this commitment commits to any state that needed for execution
        // (state root, block number, bloch hahes)
        bytes32 newStateCommitment;
        // info about processed l1 txs, l2 to l1 logs and DA
        uint256 numberOfLayer1Txs;
        bytes32 priorityOperationsHash;
        bytes32 l2LogsTreeRoot;
        address l2DaValidator; // TODO: already saved in the storage, can just add from there to PI
        bytes32 daCommitment;
        // sending used batch inputs to validate on the settlement layer
        uint64 firstBlockTimestamp;
        uint64 lastBlockTimestamp;
        uint256 chainId; // TODO: already saved in the storage, can just add from there to PI
        // extra calldata to pass to da validator
        bytes operatorDAInput;
    }

    #[derive(Debug)]

    struct Merged {
        StoredBatchInfo foo;
        CommitBoojumOSBatchInfo[] bar;
    }
}

pub fn compute_batch_outputs_hash(batch: &CommitBoojumOSBatchInfo) -> B256 {
    let mut bytes = Vec::with_capacity(32 + 8 + 8 + 20 + 32 + 32 + 32 + 32 + 32);

    // Encode chainId as 32-byte big-endian.
    {
        bytes.extend_from_slice(&batch.chainId.to_be_bytes::<32>());
    }
    // Encode firstBlockTimestamp (uint64 - 8 bytes)
    bytes.extend_from_slice(&batch.firstBlockTimestamp.to_be_bytes());
    // Encode lastBlockTimestamp (uint64 - 8 bytes)
    bytes.extend_from_slice(&batch.lastBlockTimestamp.to_be_bytes());
    // Encode l2DaValidator as 20 bytes already.
    bytes.extend_from_slice(batch.l2DaValidator.as_slice());
    // Encode daCommitment (bytes32 - 32 bytes)
    bytes.extend_from_slice(batch.daCommitment.as_slice());
    // Encode numberOfLayer1Txs as 32-byte big-endian.
    {
        bytes.extend_from_slice(&batch.numberOfLayer1Txs.to_be_bytes::<32>());
    }
    // Encode priorityOperationsHash (bytes32 - 32 bytes)
    bytes.extend_from_slice(batch.priorityOperationsHash.as_slice());
    // Encode l2LogsTreeRoot (bytes32 - 32 bytes)
    bytes.extend_from_slice(batch.l2LogsTreeRoot.as_slice());
    // Append zero upgrade tx hash (bytes32 - 32 bytes of zero)
    bytes.extend_from_slice(&[0u8; 32]);

    // Compute and return the keccak256 hash.
    keccak256(&bytes).into()
}

pub fn commit_to_stored(info: CommitBoojumOSBatchInfo) -> StoredBatchInfo {
    StoredBatchInfo {
        batchNumber: info.batchNumber,
        batchHash: info.newStateCommitment,
        indexRepeatedStorageChanges: 0,
        numberOfLayer1Txs: info.numberOfLayer1Txs,
        priorityOperationsHash: info.priorityOperationsHash,
        l2LogsTreeRoot: info.l2LogsTreeRoot,
        timestamp: U256::from(0), // For Boojum OS not used, 0
        commitment: compute_batch_outputs_hash(&info), // For Boojum OS batches we'll store batch output hash here
    }
}

pub fn get_batch_public_input(prev_batch: &StoredBatchInfo, batch: &StoredBatchInfo) -> B256 {
    let mut bytes = Vec::with_capacity(32 * 3);
    bytes.extend_from_slice(prev_batch.batchHash.as_slice());
    bytes.extend_from_slice(batch.batchHash.as_slice());
    bytes.extend_from_slice(batch.commitment.as_slice());
    keccak256(&bytes).into()
}

pub fn shift_b256_right(input: &B256) -> B256 {
    let mut bytes = [0_u8; 32];
    bytes[4..32].copy_from_slice(&input.as_slice()[0..28]);
    B256::from_slice(&bytes)
}

pub fn snark_public_input_for_range(
    batches: &HashMap<u64, StoredBatchInfo>,
    start: u64,
    end: u64,
) -> B256 {
    let mut result: Option<B256> = None;
    for i in start..=end {
        let batch = batches.get(&i).expect("Batch not found");
        let prev_batch = batches.get(&(i - 1)).expect("Previous batch not found");
        let public_input = get_batch_public_input(prev_batch, batch);
        // Snark public input is public_input >> 32.
        let snark_input = shift_b256_right(&public_input);

        match result {
            Some(ref mut res) => {
                // Combine with previous result.
                let mut combined = [0_u8; 64];
                combined[..32].copy_from_slice(&res.0);
                combined[32..].copy_from_slice(&snark_input.0);
                *res = shift_b256_right(&keccak256(&combined));
            }
            None => {
                result = Some(snark_input);
            }
        }
    }
    result.unwrap()
}

#[derive(Debug, Parser, Clone)]

struct ArgsRange {
    #[arg(long)]
    start: u64,
    #[arg(long)]
    end: u64,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Shows the current status.
    Show {},
    /// Computes public input for given batch range.
    PublicInput {
        #[clap(flatten)]
        range: ArgsRange,
    },
    /// Figures out the non-proven or executed batches, and uses fake prover to prove & execute them.
    FakeProveAndExecute {
        #[arg(long)]
        /// Address of the L2 sequencer to use for execution.
        /// If not specified, it will use the local one.
        l2_sequencer: Option<String>,
    },
    /// Takes existing SNARK proof and submits it to the contract.
    Prove {
        /// Path to the file with SNARK proof.
        #[arg(long)]
        snark_path: String,
        #[clap(flatten)]
        range: ArgsRange,

        /// If specified, the SNARK proof starts from this batch, rather than from
        /// range.start (useful if some other small proof was already submitted).
        #[arg(long)]
        snark_start: Option<u64>,
    },
    /// Will use a 'fake verifier' (if supported) - this way it doesn't have to spend time creating snark proof.
    FakeProve {
        /// Public input that shoudl be passed to the contract.
        #[arg(long)]
        public_input: String,
        #[clap(flatten)]
        range: ArgsRange,
    },
    /// Executes given range of blocks (this is the final step in the process).
    Execute {
        #[clap(flatten)]
        range: ArgsRange,

        #[arg(long)]
        /// Address of the L2 sequencer to use for execution.
        /// If not specified, it will use the local one.
        l2_sequencer: Option<String>,
    },
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    #[arg(short, long)]
    address: String,
    #[arg(short, long)]
    server_url: Option<String>,

    #[arg(long)]
    private_key: Option<String>,
}

/// Iterates backwards over blocks in chunks and prints transactions that emit the given event.
async fn iterate_blocks_for_event<P: Provider>(
    provider: P,
    contract_address: alloy::primitives::Address,
    event_topic: B256,
    chunk_size: u64,
    num_blocks_to_scan: Option<u64>,
) -> Result<HashSet<B256>, Box<dyn std::error::Error>> {
    // Get the current block number.
    let latest_block = provider.get_block_number().await?;
    let mut current_block = latest_block;
    let start_block = num_blocks_to_scan
        .map(|x| current_block.saturating_sub(x))
        .unwrap_or(0);

    let mut result: HashSet<B256> = HashSet::new();

    println!(
        "Scanning blocks from {} to {}...",
        start_block, current_block
    );
    // Loop backwards until block 0.
    while current_block > start_block {
        let from_block = current_block.saturating_sub(chunk_size) + 1;

        let filter = Filter::new()
            .from_block(from_block)
            .to_block(current_block)
            .address(contract_address)
            .event_signature(event_topic);

        // Get the matching logs.
        let logs = provider.get_logs(&filter).await?;
        for log in logs {
            println!(
                "Found event in tx: {:?} at block {:?}",
                log.transaction_hash, log.block_number
            );
            result.insert(log.transaction_hash.unwrap());
        }

        if from_block == 1 {
            break;
        }
        current_block = from_block - 1;
    }
    Ok(result)
}

/// Fetch batches that were sent in 'commit' transactions.
pub async fn fetch_batches<P: Provider + Clone>(
    provider: P,
    diamond_proxy_address: alloy::primitives::Address,
) -> (
    HashMap<u64, CommitBoojumOSBatchInfo>,
    HashMap<u64, StoredBatchInfo>,
) {
    let transactions = iterate_blocks_for_event(
        provider.clone(),
        diamond_proxy_address,
        IHyperchain::BlockCommit::SIGNATURE_HASH,
        10000,
        // scan at most 1M blocks.
        Some(1_000_000),
    )
    .await
    .unwrap();

    let mut batches = HashMap::new();
    let mut stored = HashMap::new();

    for tx_hash in transactions {
        let tx = provider.get_transaction_by_hash(tx_hash).await;
        if tx.is_err() {
            println!("Error fetching transaction {}", tx_hash);
            continue;
        }

        let tx_data = tx.unwrap().expect("Transaction not found");

        let tx = tx_data.inner.as_eip1559().unwrap().tx();

        if tx.input[..4] != IHyperchain::commitBatchesSharedBridgeCall::SELECTOR {
            println!("Skipping transaction: {}", tx_hash);
            continue;
        }

        let decoded = IHyperchain::commitBatchesSharedBridgeCall::abi_decode(tx.input()).unwrap();

        let commit_data = &decoded._3.clone()[1..];

        let ww = IHyperchain::tmpStuffCall::abi_decode_raw(commit_data).unwrap();
        for other in ww.commits {
            batches.insert(other.batchNumber.clone(), other.clone());
            stored.insert(other.batchNumber, commit_to_stored(other));
        }
        stored.insert(ww.stored.batchNumber, ww.stored);
    }
    (batches, stored)
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();
    let server = args
        .server_url
        .clone()
        .unwrap_or_else(|| "http://localhost:8545".to_string());

    let signer = if let Some(private_key) = args.private_key.clone() {
        let signer: PrivateKeySigner = private_key.parse().unwrap();
        signer
    } else {
        PrivateKeySigner::random()
    };

    let dry_run = args.private_key.is_none();

    let provider = ProviderBuilder::new()
        .wallet(signer)
        .connect(&server)
        .await
        .unwrap();

    let address = Address::from_hex(args.address.clone()).unwrap();

    let contract = IHyperchain::new(address, provider.clone());

    let (batches, stored) = fetch_batches(provider.clone(), address).await;

    match args.command {
        Command::Show {} => {
            let total_committed = contract.getTotalBatchesCommitted().call().await.unwrap();
            let total_verified = contract.getTotalBatchesVerified().call().await.unwrap();
            let total_executed = contract.getTotalBatchesExecuted().call().await.unwrap();
            let semver = contract.getSemverProtocolVersion().call().await.unwrap();
            println!("Using diamond Proxy: {}", args.address);
            println!(
                "Using Verifier: {}",
                contract.getVerifier().call().await.unwrap()
            );
            println!("Total batches committed: {}", total_committed);
            println!("Total batches verified: {}", total_verified);
            println!("Total batches executed: {}", total_executed);
            println!("Batches recovered from sequencer: {}", batches.len());

            println!(
                "Protocol version: {}.{}.{}",
                semver._0, semver._1, semver._2
            );
        }
        Command::PublicInput { range } => {
            let start = range.start;
            let end = range.end;

            if start > end {
                panic!("Start must be less than or equal to end");
            }

            for i in start..=end {
                let batch = stored.get(&i).expect("Batch not found");
                let prev_batch = stored.get(&(i - 1)).expect("Previous batch not found");
                let public_input = get_batch_public_input(prev_batch, batch);
                let snark_public_input = shift_b256_right(&public_input);
                println!("FRI Public input for batch {}: {}", i, public_input);
                println!("SNARK Public input for batch {}: {}", i, snark_public_input);
            }
            println!(
                "Snark public input for range {}-{}: {}",
                start,
                end,
                snark_public_input_for_range(&stored, start, end)
            );
        }
        Command::Prove {
            snark_path,
            range,
            snark_start,
        } => {
            prove_batches(
                contract,
                range.start,
                range.end,
                &stored,
                snark_start,
                snark_path,
                dry_run,
            )
            .await
        }
        Command::FakeProve {
            public_input,
            range,
        } => {
            fake_prove_batches(
                contract,
                range.start,
                range.end,
                &stored,
                public_input,
                dry_run,
            )
            .await
        }
        Command::Execute {
            range,
            l2_sequencer,
        } => {
            let l2_sequencer = l2_sequencer.unwrap_or_else(|| match args.server_url {
                Some(_) => panic!("You set --server-url, so you must specify --l2-sequencer"),
                None => "http://localhost:3050".to_string(),
            });
            execute_batches(
                contract,
                range.start,
                range.end,
                &l2_sequencer,
                &stored,
                dry_run,
            )
            .await;
        }
        Command::FakeProveAndExecute { l2_sequencer } => {
            let l2_sequencer = l2_sequencer.unwrap_or_else(|| match args.server_url {
                Some(_) => panic!("You set --server-url, so you must specify --l2-sequencer"),
                None => "http://localhost:3050".to_string(),
            });
            if dry_run {
                panic!("please provide --private-key to run this command");
            }

            let total_committed = contract
                .getTotalBatchesCommitted()
                .call()
                .await
                .unwrap()
                .try_into()
                .unwrap();
            let total_verified: u64 = contract
                .getTotalBatchesVerified()
                .call()
                .await
                .unwrap()
                .try_into()
                .unwrap();
            let total_executed: u64 = contract
                .getTotalBatchesExecuted()
                .call()
                .await
                .unwrap()
                .try_into()
                .unwrap();

            if total_committed != total_verified {
                println!(
                    "Fake proving from {} to {}",
                    total_verified + 1,
                    total_committed
                );
                let public_input =
                    snark_public_input_for_range(&stored, total_verified + 1, total_committed);
                fake_prove_batches(
                    contract.clone(),
                    total_verified + 1,
                    total_committed,
                    &stored,
                    public_input.to_string(),
                    dry_run,
                )
                .await;
            }

            if total_executed != total_committed {
                println!(
                    "Executing from {} to {}",
                    total_executed + 1,
                    total_committed
                );
                execute_batches(
                    contract,
                    total_executed + 1,
                    total_committed,
                    &l2_sequencer,
                    &stored,
                    dry_run,
                )
                .await;
                println!(
                    "\x1b[32mAll batches (up to batch {}) proven and executed\x1b[0m",
                    total_committed
                );
            } else {
                println!("Nothing to execute, all batches are executed already");
            }
        }
    };
}
