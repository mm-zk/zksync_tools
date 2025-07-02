use std::collections::{HashMap, HashSet};

use alloy::{
    primitives::{Address, B256, U256, keccak256},
    signers::local::PrivateKeySigner,
};
use clap::Parser;

use alloy::{
    consensus::Transaction,
    hex::FromHex,
    providers::{Provider, ProviderBuilder},
    rpc::types::Filter,
    sol,
    sol_types::{SolCall, SolEvent},
};

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

        function proveBatchesSharedBridge(
            uint256, // _chainId
            uint256 _processBatchFrom,
            uint256 _processBatchTo,
            bytes calldata _proofData
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

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    address: String,
    #[arg(short, long)]
    server_url: Option<String>,

    #[arg(long)]
    start: u64,
    #[arg(long)]
    end: u64,

    #[arg(long)]
    snark_path: String,

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

#[tokio::main]
async fn main() {
    let args = Cli::parse();
    let server = args
        .server_url
        .unwrap_or_else(|| "http://localhost:8545".to_string());
    println!("Diamond Proxy: {}", args.address);
    let provider = ProviderBuilder::new().connect(&server).await.unwrap();

    let address = Address::from_hex(args.address).unwrap();

    let contract = IHyperchain::new(address, provider.clone());

    println!("Verifier: {}", contract.getVerifier().call().await.unwrap());

    let transactions = iterate_blocks_for_event(
        provider.clone(),
        address,
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
        let tx_data = provider
            .get_transaction_by_hash(tx_hash)
            .await
            .unwrap()
            .expect("Transaction not found");

        let tx = tx_data.inner.as_eip1559().unwrap().tx();

        if tx.input[..4] != IHyperchain::commitBatchesSharedBridgeCall::SELECTOR {
            println!("Skipping transaction: {}", tx_hash);
            continue;
        }

        let decoded = IHyperchain::commitBatchesSharedBridgeCall::abi_decode(tx.input()).unwrap();

        let commit_data = &decoded._3.clone()[1..];
        //println!("commit data: 0x{}", hex::encode(commit_data));
        //let ww = IHyperchain::tmpStuffCall::abi_decode_raw(commit_data).unwrap();

        let ww = IHyperchain::tmpStuffCall::abi_decode_raw(commit_data).unwrap();
        for other in ww.commits {
            batches.insert(other.batchNumber.clone(), other.clone());
            stored.insert(other.batchNumber, commit_to_stored(other));
        }
        stored.insert(ww.stored.batchNumber, ww.stored);
    }

    println!("Got {} batches", batches.len());
    let data = snark::load_snark_from_file(&args.snark_path).unwrap();
    let mut proof: Vec<U256> = data
        .iter()
        .map(|x| U256::from_str_radix(x, 10).unwrap())
        .collect();

    // ohbender type
    proof.insert(0, U256::from(2));

    let prev_batch = args.start - 1;

    // FRI from batch 1.
    proof.insert(
        1,
        U256::from(0),
        /*get_batch_public_input(
            &stored.get(&(prev_batch - 1)).unwrap(),
            &stored.get(&prev_batch).unwrap(),
        )
        .into(),*/
    );

    let mut proof_data = vec![0u8];

    let new_batches = (args.start..=args.end)
        .map(|x| stored.get(&x).unwrap().clone())
        .collect();

    let proof_payload = IHyperchain::proofPayloadCall {
        old: stored.get(&prev_batch).unwrap().clone(),
        newInfo: new_batches,
        proof,
    };

    proof_payload.abi_encode_raw(&mut proof_data);

    let _ = contract
        .proveBatchesSharedBridge(
            0.try_into().unwrap(),
            args.start.try_into().unwrap(),
            args.end.try_into().unwrap(),
            proof_data.clone().into(),
        )
        .call()
        .await
        .unwrap();

    if let Some(private_key) = args.private_key {
        let signer: PrivateKeySigner = private_key.parse().unwrap();
        let provider = ProviderBuilder::new()
            .wallet(signer)
            .connect(&server)
            .await
            .unwrap();
        let contract = IHyperchain::new(address, provider.clone());

        let tx = contract
            .proveBatchesSharedBridge(
                0.try_into().unwrap(),
                args.start.try_into().unwrap(),
                args.end.try_into().unwrap(),
                proof_data.into(),
            )
            .max_priority_fee_per_gas(2_000_000_002)
            .max_fee_per_gas(2_000_000_002)
            .send()
            .await
            .unwrap();
        println!("Transaction sent: {}", tx.tx_hash());
        let receipt = tx.get_receipt().await.unwrap();
        if receipt.status() {
            println!("Transaction succeeded: {:?}", receipt);
        } else {
            println!("Transaction failed: {:?}", receipt);
        }
    }
}
