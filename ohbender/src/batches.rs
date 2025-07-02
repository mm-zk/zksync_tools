use std::collections::{HashMap, HashSet};

use alloy::primitives::{Address, B256, U256, keccak256};

use alloy::{
    consensus::Transaction,
    hex::FromHex,
    providers::{Provider, ProviderBuilder},
    rpc::types::Filter,
    sol,
    sol_types::{SolCall, SolEvent},
};

sol! {
    #[sol(rpc)]
    contract IHyperchain {

        function commitBatchesSharedBridge(uint256,uint256,uint256,bytes commitData);

        function commitDataPieces(StoredBatchInfo stored, CommitBoojumOSBatchInfo[] commits) external;

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

/// Scans over the events for a given diamond proxy - to get the commit hashes for all batches starting from start_batch.
pub async fn fetch_commit_hashes_up_to_batch<P: Provider>(
    provider: P,
    diamond_proxy: Address,
    start_batch: u64,
) -> Result<HashMap<u64, StoredBatchInfo>, Box<dyn std::error::Error>> {
    let event_topic = IHyperchain::BlockCommit::SIGNATURE_HASH;

    // Get the current block number.
    let mut current_block = provider.get_block_number().await?;

    let mut result = HashMap::new();
    let chunk_size = 10_000;

    // Loop backwards until block 0.
    loop {
        let from_block = current_block.saturating_sub(chunk_size) + 1;

        let filter = Filter::new()
            .from_block(from_block)
            .to_block(current_block)
            .address(diamond_proxy)
            .event_signature(event_topic);

        // Get the matching logs.
        let logs = provider.get_logs(&filter).await?;
        for log in logs {
            println!(
                "Found event in tx: {:?} at block {:?}",
                log.transaction_hash, log.block_number
            );
            let tx_hash = log.transaction_hash.unwrap();
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

            let decoded =
                IHyperchain::commitBatchesSharedBridgeCall::abi_decode(tx.input()).unwrap();

            let commit_data_parsed =
                IHyperchain::commitDataPiecesCall::abi_decode_raw(&decoded.commitData.clone()[1..])
                    .unwrap();
            for other in commit_data_parsed.commits {
                result.insert(other.batchNumber, commit_to_stored(other));
            }
            result.insert(
                commit_data_parsed.stored.batchNumber,
                commit_data_parsed.stored,
            );
        }

        if from_block == 1 {
            break;
        }
        if result.contains_key(&start_batch) {
            break;
        }
        current_block = from_block - 1;
    }
    Ok(result)
}

pub fn create_ohbender_proof_payload(
    batches: &HashMap<u64, StoredBatchInfo>,
    serialized_proof: Vec<String>,
    batch_from: u64,
    batch_to: u64,
) -> Vec<u8> {
    let prev_batch = batch_from.checked_sub(1).unwrap();
    // First create the proof itself.
    let mut proof: Vec<U256> = serialized_proof
        .iter()
        .map(|x| U256::from_str_radix(x, 10).unwrap())
        .collect();
    // ohbender type
    proof.insert(0, U256::from(2));
    proof.insert(
        1,
        // If the proof went 'beyond' the range (so starting earlier than batch_from),
        // this field would be the rolling hash of entries from its start until the batch_from - 1.
        // Here we assume that proofs are exactly matching the batch_from -> batch_to - so this can be 0.
        U256::from(0),
    );

    // Now let's create proof_data.
    let mut proof_data = vec![0u8];

    let new_batches = (batch_from..=batch_to)
        .map(|x| batches.get(&x).unwrap().clone())
        .collect();

    let proof_payload = IHyperchain::proofPayloadCall {
        old: batches.get(&prev_batch).unwrap().clone(),
        newInfo: new_batches,
        proof,
    };

    proof_payload.abi_encode_raw(&mut proof_data);
    proof_data
}

pub async fn call_prove_batches_shared_bridge<P: Provider>(
    provider: P,
    diamond_proxy: Address,
    batch_from: u64,
    batch_to: u64,
    proof_payload: Vec<u8>,
) {
    let contract = IHyperchain::new(diamond_proxy, provider);

    let _ = contract
        .proveBatchesSharedBridge(
            0.try_into().unwrap(),
            batch_from.try_into().unwrap(),
            batch_to.try_into().unwrap(),
            proof_payload.into(),
        )
        .call()
        .await
        .unwrap();
}
