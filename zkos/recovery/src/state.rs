// Things related with state (tree etc).
use std::collections::BTreeMap;

use alloy::primitives::B256;
use blake2::{Blake2s256, Digest};

use crate::state_genesis::GenesisState;

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
