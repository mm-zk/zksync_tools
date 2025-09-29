// ok, I'll need 'GenesisState' first.

// I might also need a simple tree..

use std::collections::BTreeMap;

use alloy::{
    consensus::{EMPTY_OMMER_ROOT_HASH, Header},
    eips::eip1559::INITIAL_BASE_FEE,
    primitives::{Address, B64, B256, Bloom, U256},
};
use blake2::{Blake2s256, Digest};
use serde::{Deserialize, Serialize};
use zk_os_api::helpers::{set_properties_code, set_properties_nonce};
use zk_os_basic_system::system_implementation::flat_storage_model::{
    ACCOUNT_PROPERTIES_STORAGE_ADDRESS, AccountProperties,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct GenesisInput {
    /// Initial contracts to deploy in genesis.
    /// Storage entries that set the contracts as deployed and preimages will be derived from this field.
    pub initial_contracts: Vec<(Address, alloy::primitives::Bytes)>,
    /// Additional (not related to contract deployments) storage entries to add in genesis state.
    pub additional_storage: Vec<(B256, B256)>,
}

// load string from genesis.json and then parse.

#[derive(Debug, Clone)]
pub struct GenesisState {
    /// Storage logs for the genesis block.
    pub storage_logs: Vec<(B256, B256)>,
    /// Preimages of the padded bytecodes with artifacts and hashes of account properties
    /// for the contracts deployed in the genesis block.
    /// Note: these preimages don't include `force_deploy_preimages` -
    /// see `genesis_upgrade_tx` method for details
    pub preimages: Vec<(B256, Vec<u8>)>,
    /// The header of the genesis block.
    pub header: Header,
}

pub fn init_genesis() -> GenesisState {
    let genesis_json = include_str!("genesis.json");
    let genesis_input: GenesisInput = serde_json::from_str(genesis_json).unwrap();

    // BTreeMap is used to ensure that the storage logs are sorted by key, so that the order is deterministic
    // which is important for tree.
    let mut storage_logs: BTreeMap<B256, B256> = BTreeMap::new();
    let mut preimages = vec![];

    for (address, deployed_code) in genesis_input.initial_contracts {
        let mut account_properties = AccountProperties::default();
        // When contracts are deployed, they have a nonce of 1.
        set_properties_nonce(&mut account_properties, 1);
        let bytecode_preimage = set_properties_code(&mut account_properties, &deployed_code);
        let bytecode_hash = account_properties.bytecode_hash;

        let flat_storage_key = {
            let mut bytes = [0u8; 64];
            bytes[12..32].copy_from_slice(&ACCOUNT_PROPERTIES_STORAGE_ADDRESS.to_be_bytes::<20>());
            bytes[44..64].copy_from_slice(address.as_slice());

            B256::from_slice(Blake2s256::digest(bytes).as_slice())
        };
        let account_properties_hash = account_properties.compute_hash();
        storage_logs.insert(
            flat_storage_key,
            account_properties_hash.as_u8_array().into(),
        );

        preimages.push((bytecode_hash.as_u8_array().into(), bytecode_preimage));
        preimages.push((
            account_properties_hash.as_u8_array().into(),
            account_properties.encoding().to_vec(),
        ));
    }

    for (key, value) in genesis_input.additional_storage {
        let duplicate = storage_logs.insert(key, value).is_some();
        if duplicate {
            panic!("Genesis input contains duplicate storage key: {key:?}");
        }
    }

    let header = Header {
        parent_hash: B256::ZERO,
        ommers_hash: EMPTY_OMMER_ROOT_HASH,
        beneficiary: Address::ZERO,
        // for now state root is zero
        state_root: B256::ZERO,
        transactions_root: B256::ZERO,
        receipts_root: B256::ZERO,
        logs_bloom: Bloom::ZERO,
        difficulty: U256::ZERO,
        number: 0,
        gas_limit: 5_000,
        gas_used: 0,
        timestamp: 0,
        extra_data: Default::default(),
        mix_hash: B256::ZERO,
        nonce: B64::ZERO,
        base_fee_per_gas: Some(INITIAL_BASE_FEE),
        withdrawals_root: None,
        blob_gas_used: None,
        excess_blob_gas: None,
        parent_beacon_block_root: None,
        requests_hash: None,
    };

    GenesisState {
        storage_logs: storage_logs.into_iter().collect(),
        preimages,
        header,
    }
}

#[derive(Debug, Clone, Copy, Default)]
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

pub fn init_tree_genesis() -> LocalTree {
    let genesis = init_genesis();

    let mut tree = LocalTree::new();

    for (key, value) in genesis.storage_logs {
        tree.add_entry(key, value);
    }

    tree
}

pub fn compute_genesis_commitment() -> B256 {
    let genesis = init_genesis();

    let mut tree = LocalTree::new();

    for (key, value) in genesis.storage_logs {
        tree.add_entry(key, value);
    }

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

fn hash_leaf(leaf: &Leaf) -> B256 {
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

fn compress(lhs: &B256, rhs: &B256) -> B256 {
    let mut hasher = Blake2s256::new();
    hasher.update(lhs);
    hasher.update(rhs);
    B256::from(<[u8; 32]>::from(hasher.finalize()))
}

#[cfg(test)]
mod tests {
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
        let tree = init_tree_genesis();
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
        let commitment = compute_genesis_commitment();
        println!("Genesis commitment: {:#x}", commitment);

        let expected_commitment: B256 =
            "0xc346a158cce093e99ab65a95c884a26629d0e4f8d00ae20bbca4bfc4b204eec2"
                .parse()
                .unwrap();
        assert_eq!(commitment, expected_commitment);
    }
}
