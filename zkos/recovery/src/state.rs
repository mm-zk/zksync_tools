// Things related with state (tree etc).
use std::collections::BTreeMap;

use alloy::primitives::{Address, B256, Keccak256, U256, address};
use blake2::{Blake2s256, Digest};
use serde::{Deserialize, Serialize};
use zk_os_basic_system::system_implementation::flat_storage_model::AccountProperties;

use crate::{
    BatchInfo, BlockInfo,
    chain_genesis::GenesisUpgradeLocalInfo,
    contracts::{CommitBatchInfoZKsyncOS, StoredBatchInfo},
    state_genesis::GenesisState,
    statediffs::{self, ValueDiff},
};
/// Struct describing full blockchain state.
#[derive(Serialize, Deserialize, Debug)]
pub struct BlockchainState {
    pub genesis_tx: GenesisUpgradeLocalInfo,
    pub genesis_state: GenesisState,
    pub tree: LocalTree,
    pub preimage_store: BTreeMap<B256, Vec<u8>>,
    pub current_block: u64,
    pub current_batch: u64,
    pub last_256_block_hashes: Vec<B256>,

    pub batches_metadata: Vec<BatchMetadata>,
}

impl BlockchainState {
    pub fn new(genesis_state: GenesisState, genesis_tx: GenesisUpgradeLocalInfo) -> Self {
        let tree = init_tree_genesis(&genesis_state);

        let preimage_store = BTreeMap::from_iter(genesis_state.preimages.iter().cloned());

        let mut last_256_block_hashes = vec![B256::default(); 256];
        last_256_block_hashes[255] = genesis_state.header.hash_slow();

        Self {
            genesis_tx,
            genesis_state,
            tree,
            preimage_store,
            current_batch: 0,
            current_block: 0,
            last_256_block_hashes,
            batches_metadata: vec![],
        }
    }

    pub fn apply_batch(&mut self, batch_number: u64, info: &BatchInfo) {
        assert_eq!(batch_number, self.current_batch + 1);
        self.current_batch = batch_number;

        // For now, only support cases with single batch per commit.
        assert_eq!(1, info.commits.len());
        let commit = &info.commits[0];
        let start_block = self.current_block;
        for block_info in &info.blocks_data {
            self.apply_block(block_info);
        }

        // Now compute commitments.

        let tree_root = self.tree.compute_root();
        let leaf_count: u64 = self.tree.leaves.len() as u64;

        tracing::debug!("Tree root: 0x{}", hex::encode(tree_root));

        tracing::debug!(
            "Expected state commitment: 0x{}",
            hex::encode(commit.newStateCommitment.as_slice())
        );
        let mut hasher = Blake2s256::new();
        hasher.update(tree_root.as_slice());
        hasher.update(leaf_count.to_be_bytes());
        hasher.update(self.current_block.to_be_bytes());
        tracing::debug!("Block number used: {}", self.current_block);

        let mut blocks_hasher = Blake2s256::new();
        for h in self.last_256_block_hashes.iter() {
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
        // Safety check that commitment matches.
        assert_eq!(
            commit.newStateCommitment, state_commitment,
            "State commitment mismatch on batch {}",
            batch_number
        );

        /*if batch_number > 2 {
            // HACK HACK HACK -- we ignore genesis (I'm lazy) and we ignore batches with ugprades (and that's serious).
            // We're missing info about upgrade tx.
            assert_eq!(self.last_stored_batch_info, info.stored);
        }

        // now convert 'commit' into batch info
        self.last_stored_batch_info = commit_batch_info_to_stored_batch_info(commit);*/

        self.batches_metadata.push(BatchMetadata {
            previous_stored_batch_info: FriendlyStoredBatchInfo {
                batch_number: info.stored.batchNumber,
                state_commitment: info.stored.batchHash,
                number_of_layer1_txs: info.stored.numberOfLayer1Txs.try_into().unwrap(),
                priority_operations_hash: info.stored.priorityOperationsHash,
                dependency_roots_rolling_hash: info.stored.dependencyRootsRollingHash,
                l2_to_l1_logs_root_hash: info.stored.l2LogsTreeRoot,
                commitment: info.stored.commitment,
                // this is not really used.
                last_block_timestamp: commit.lastBlockTimestamp,
            },
            commit_batch_info: CommitBatchInfo {
                batch_number: commit.batchNumber,
                new_state_commitment: commit.newStateCommitment,
                number_of_layer1_txs: commit.numberOfLayer1Txs.try_into().unwrap(),
                priority_operations_hash: commit.priorityOperationsHash,
                dependency_roots_rolling_hash: commit.dependencyRootsRollingHash,
                l2_to_l1_logs_root_hash: commit.l2LogsTreeRoot,
                l2_da_validator: commit.l2DaValidator,
                da_commitment: commit.daCommitment,
                first_block_timestamp: commit.firstBlockTimestamp,
                last_block_timestamp: commit.lastBlockTimestamp,
                chain_id: commit.chainId.try_into().unwrap(),
                chain_address: Address::ZERO, // HACK HACK HACK -- we don't have this info
                operator_da_input: commit.operatorDAInput.to_vec(),
                upgrade_tx_hash: None, // HACK HACK HACK -- we don't have this info
            },
            first_block_number: start_block + 1,
            last_block_number: self.current_block,
            tx_count: 15, // HACK HACK HACK -- we don't have this info
        });
    }

    pub fn apply_block(&mut self, block_info: &BlockInfo) {
        tracing::debug!(
            "  Block: 0x{} with {} state diffs and {} logs",
            hex::encode(block_info.block_hash.as_slice()),
            block_info.state_diffs.len(),
            block_info.logs.len()
        );
        apply_block_state_diffs(
            &mut self.tree,
            &mut self.preimage_store,
            block_info,
            &self.genesis_tx,
        );

        for i in 0..255 {
            self.last_256_block_hashes[i] = self.last_256_block_hashes[i + 1];
        }

        self.last_256_block_hashes[255] = block_info.block_hash;
        self.current_block += 1;
    }
}

pub fn apply_block_state_diffs(
    tree: &mut LocalTree,
    preimage_store: &mut BTreeMap<B256, Vec<u8>>,
    info: &BlockInfo,
    genesis_info: &GenesisUpgradeLocalInfo, // In future, this should also cover upgraded and l1 tx.
) {
    let mut force_deploy_map = BTreeMap::new();
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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Leaf {
    pub key: B256,
    pub value: B256,
    /// 0-based index of a leaf with the lexicographically next key.
    pub next_index: u64,
}

impl Leaf {
    /// Minimum guard leaf inserted at the tree at its initialization.
    pub const MIN_GUARD: Self = Self {
        key: B256::ZERO,
        value: B256::ZERO,
        next_index: 1,
    };

    /// Maximum guard leaf inserted at the tree at its initialization.
    pub const MAX_GUARD: Self = Self {
        key: B256::repeat_byte(0xff),
        value: B256::ZERO,
        // Circular pointer to self; never updated.
        next_index: 1,
    };
}

const TREE_DEPTH: u8 = 64;

#[derive(Serialize, Deserialize, Debug)]
/// Simple in-memory implementation of a Merkle tree with linked list of leaves.
pub struct LocalTree {
    // Key to index.
    pub sorted_leaves: BTreeMap<B256, u64>,
    pub leaves: Vec<Leaf>,
}

impl LocalTree {
    pub fn new() -> Self {
        let sorted_leaves = BTreeMap::from([(B256::ZERO, 0), (B256::repeat_byte(0xff), 1)]);

        let leaves = vec![Leaf::MIN_GUARD, Leaf::MAX_GUARD];
        Self {
            sorted_leaves,
            leaves,
        }
    }

    pub fn add_entry(&mut self, key: B256, value: B256) {
        if self.sorted_leaves.contains_key(&key) {
            // get index & update value.
            let index = self.sorted_leaves.get_mut(&key).unwrap();
            self.leaves[*index as usize].value = value;
            return;
        }

        let index = self.sorted_leaves.len() as u64;
        self.sorted_leaves.insert(key, index);

        // Next index must exist, as we have MAX_GUARD.
        let next_index = *self.sorted_leaves.range(key..).nth(1).unwrap().1;
        let prev_index = *self.sorted_leaves.range(..key).rev().nth(0).unwrap().1;

        let leaf = Leaf {
            key,
            value,
            next_index,
        };
        self.leaves.push(leaf);
        self.leaves[prev_index as usize].next_index = index;
    }

    pub fn get_value(&self, key: B256) -> B256 {
        if let Some(index) = self.sorted_leaves.get(&key) {
            self.leaves[*index as usize].value
        } else {
            B256::ZERO
        }
    }

    pub fn compute_root(&self) -> B256 {
        let mut current_level = self
            .leaves
            .iter()
            .map(|leaf| hash_leaf(leaf))
            .collect::<Vec<_>>();

        let mut current_zero = hash_leaf(&Leaf::default());

        for _ in 0..TREE_DEPTH {
            let next_level_size = current_level.len() / 2 + (current_level.len() % 2);

            let mut next_level = vec![];

            for i in 0..next_level_size {
                next_level.push(compress(
                    &current_level[i * 2],
                    current_level.get(i * 2 + 1).unwrap_or(&current_zero),
                ));
            }
            current_zero = compress(&current_zero, &current_zero);
            current_level = next_level;
        }

        assert!(current_level.len() == 1);
        current_level[0]
    }
}

pub fn init_tree_genesis(genesis: &GenesisState) -> LocalTree {
    let mut tree = LocalTree::new();

    for (key, value) in &genesis.storage_logs {
        tree.add_entry(key.clone(), value.clone());
    }

    tree
}

pub fn compute_genesis_commitment(genesis: &GenesisState) -> B256 {
    let tree = init_tree_genesis(genesis);

    let genesis_root = tree.compute_root();

    let number = 0u64;
    let timestamp = 0u64;

    let last_256_block_hashes_blake = {
        let mut blocks_hasher = Blake2s256::new();
        for _ in 0..255 {
            blocks_hasher.update([0u8; 32]);
        }
        blocks_hasher.update(genesis.header.hash_slow());

        blocks_hasher.finalize()
    };

    let leaf_count: u64 = tree.leaves.len() as u64;

    let mut hasher = Blake2s256::new();
    hasher.update(genesis_root);
    hasher.update(leaf_count.to_be_bytes());
    hasher.update(number.to_be_bytes());
    hasher.update(last_256_block_hashes_blake);
    hasher.update(timestamp.to_be_bytes());
    let state_commitment = B256::from_slice(&hasher.finalize());

    state_commitment
}

// Blake2Hasher.

pub fn hash_leaf(leaf: &Leaf) -> B256 {
    let mut hashed_bytes = [0; 2 * 32 + 8];
    hashed_bytes[..32].copy_from_slice(leaf.key.as_slice());
    hashed_bytes[32..64].copy_from_slice(leaf.value.as_slice());
    hashed_bytes[64..].copy_from_slice(&leaf.next_index.to_le_bytes());
    hash_bytes(&hashed_bytes)
}

fn hash_bytes(value: &[u8]) -> B256 {
    let mut hasher = Blake2s256::new();
    hasher.update(value);
    B256::from(<[u8; 32]>::from(hasher.finalize()))
}

pub fn compress(lhs: &B256, rhs: &B256) -> B256 {
    let mut hasher = Blake2s256::new();
    hasher.update(lhs);
    hasher.update(rhs);
    B256::from(<[u8; 32]>::from(hasher.finalize()))
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

#[cfg(test)]
mod tests {
    use alloy::primitives::U256;

    use crate::state_genesis::init_genesis;

    use super::*;
    #[test]
    fn test_local_tree() {
        let mut tree = LocalTree::new();

        let key1 = B256::repeat_byte(1);
        let key2 = B256::repeat_byte(2);
        let key3 = B256::repeat_byte(3);
        tree.add_entry(key2, key2);

        tree.add_entry(key1, key1);
        tree.add_entry(key3, key3);

        assert_eq!(tree.leaves.len(), 5);
        assert_eq!(tree.leaves[0].next_index, 3);
        assert_eq!(tree.leaves[3].next_index, 2);
        assert_eq!(tree.leaves[2].next_index, 4);
    }

    #[test]
    fn compute_genesis() {
        let genesis_state = init_genesis();
        let tree = init_tree_genesis(&genesis_state);
        let root = tree.compute_root();
        println!("Genesis root: {:#x}", root);
    }

    // empty test
    #[test]
    fn empty_tree() {
        let tree = LocalTree::new();
        let root = tree.compute_root();
        let expected_root_hash: B256 =
            "0x90a83ead2ba2194fbbb0f7cd2a017e36cfb4891513546d943a7282c2844d4b6b"
                .parse()
                .unwrap();
        assert_eq!(root, expected_root_hash);
    }

    // Test with 1 insertion
    #[test]
    fn two_insertions() {
        let mut tree = LocalTree::new();

        tree.add_entry(U256::from(0xc0ffeefeu32).into(), B256::repeat_byte(0x10));
        tree.add_entry(U256::from(0xdeadbeefu32).into(), B256::repeat_byte(0x20));

        let root = tree.compute_root();
        let expected_root_hash: B256 =
            "0xc90465eddad7cc858a2fbf61013d7051c143887a887e5a7a19344ac32151b207"
                .parse()
                .unwrap();
        assert_eq!(root, expected_root_hash);
    }

    // print current state commitment
    #[test]
    fn print_genesis_commitment() {
        let genesis_state = init_genesis();
        let commitment = compute_genesis_commitment(&genesis_state);
        println!("Genesis commitment: {:#x}", commitment);

        let expected_commitment: B256 =
            "0xc346a158cce093e99ab65a95c884a26629d0e4f8d00ae20bbca4bfc4b204eec2"
                .parse()
                .unwrap();
        assert_eq!(commitment, expected_commitment);
    }
}

pub fn commit_batch_info_to_stored_batch_info(commit: &CommitBatchInfoZKsyncOS) -> StoredBatchInfo {
    let commitment = {
        let mut hasher = Keccak256::new();
        hasher.update(U256::from(commit.chainId).to_be_bytes::<32>());
        hasher.update(&commit.firstBlockTimestamp.to_be_bytes());
        hasher.update(&commit.lastBlockTimestamp.to_be_bytes());
        hasher.update(commit.l2DaValidator.0.0);
        hasher.update(commit.daCommitment.0);
        hasher.update(commit.numberOfLayer1Txs.to_be_bytes::<32>());
        hasher.update(commit.priorityOperationsHash.0);
        hasher.update(commit.l2LogsTreeRoot.0);
        //FIXME: hasher.update(commit.upgradeTxHash.as_u8_ref()); -- issue!!
        hasher.update(B256::ZERO.0); // upgradeTxHash is zero in zkSync OS
        hasher.update(commit.dependencyRootsRollingHash.0);
        hasher.finalize()
    };

    // Based of zksync-os-server l1-sender commitment.rs
    StoredBatchInfo {
        batchNumber: commit.batchNumber,
        batchHash: commit.newStateCommitment,
        indexRepeatedStorageChanges: 0u64, // not used
        numberOfLayer1Txs: commit.numberOfLayer1Txs,
        priorityOperationsHash: commit.priorityOperationsHash,
        dependencyRootsRollingHash: commit.dependencyRootsRollingHash,
        l2LogsTreeRoot: commit.l2LogsTreeRoot, //??
        timestamp: U256::from(0),              // not used
        commitment,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FriendlyStoredBatchInfo {
    pub batch_number: u64,
    pub state_commitment: B256,
    pub number_of_layer1_txs: u64,
    pub priority_operations_hash: B256,
    pub dependency_roots_rolling_hash: B256,
    pub l2_to_l1_logs_root_hash: B256,
    pub commitment: B256,
    pub last_block_timestamp: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CommitBatchInfo {
    pub batch_number: u64,
    pub new_state_commitment: B256,
    pub number_of_layer1_txs: u64,
    pub priority_operations_hash: B256,
    pub dependency_roots_rolling_hash: B256,
    pub l2_to_l1_logs_root_hash: B256,
    pub l2_da_validator: Address,
    pub da_commitment: B256,
    pub first_block_timestamp: u64,
    pub last_block_timestamp: u64,
    pub chain_id: u64,
    pub chain_address: Address,
    pub operator_da_input: Vec<u8>,
    pub upgrade_tx_hash: Option<B256>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchMetadata {
    pub previous_stored_batch_info: FriendlyStoredBatchInfo,
    pub commit_batch_info: CommitBatchInfo,
    pub first_block_number: u64,
    pub last_block_number: u64,
    pub tx_count: usize,
}
