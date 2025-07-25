// Things related to the L1 message merkle tree.
// This code is not optimized, and some pieces are copied from the mini_merkle_tree crate from zksync-era.

use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
};

use alloy::{
    hex::FromHex,
    primitives::{Address, B256},
    providers::ProviderBuilder,
};

use crate::IHyperchain;

pub struct MerkleInfoForExecute {
    last_block_number: u64,
    current_size: u64,
    block_range: HashMap<u64, (u64, u64)>,
    merkle_tree: MiniMerkleTree,
}

impl MerkleInfoForExecute {
    pub fn init(l1_txs: &HashMap<u64, Vec<B256>>) -> Self {
        dbg!(hash_bytes(&[]));

        let mut merkle_info = MerkleInfoForExecute {
            last_block_number: 0,
            current_size: 0,
            merkle_tree: MiniMerkleTree::new(),
            block_range: Default::default(),
        };

        let mut keys: Vec<u64> = l1_txs.keys().copied().collect();
        keys.sort_unstable();
        for key in keys {
            let txs = l1_txs.get(&key).unwrap().to_vec();
            merkle_info.add_block_l1_txs(key, txs);
        }
        merkle_info
    }

    pub fn add_block_l1_txs(&mut self, block_number: u64, l1_txs: Vec<B256>) {
        assert!(
            block_number > self.last_block_number,
            "Block numbers must be added in ascending order"
        );
        self.last_block_number = block_number;

        let size_before = self.current_size;

        for tx_hash in &l1_txs {
            self.merkle_tree.push_hash(tx_hash.clone());
        }
        self.current_size += l1_txs.len() as u64;

        println!(
            "XXX: block: {}, range: {} {}",
            block_number, size_before, self.current_size
        );

        self.block_range
            .insert(block_number, (size_before, self.current_size));
    }

    pub fn get_merkle_path_for_l1_tx_in_block(&self, block_number: u64) -> (Vec<B256>, Vec<B256>) {
        let range = self
            .block_range
            .get(&block_number)
            .expect("Block number not found in the range");
        let mut left_path = vec![];
        let mut right_path = vec![];

        self.merkle_tree
            .compute_merkle_root_and_path(range.0 as usize, Some(&mut left_path), None);
        let root = self.merkle_tree.compute_merkle_root_and_path(
            (range.1 - 1) as usize,
            Some(&mut right_path),
            None,
        );

        dbg!(root);

        (
            left_path.iter().map(|x| x.unwrap()).collect(),
            right_path.iter().map(|x| x.unwrap()).collect(),
        )
    }
}

#[derive(Debug, Clone)]
pub struct MiniMerkleTree {
    /// Stores untrimmed (uncached) leaves of the tree.
    hashes: VecDeque<B256>,
    /// Size of the tree. Always a power of 2.
    /// If it is greater than `self.start_index + self.hashes.len()`, the remaining leaves are empty.
    binary_tree_size: usize,
    /// Index of the leftmost untrimmed leaf.
    start_index: usize,
    /// Left subset of the Merkle path to the first untrimmed leaf (i.e., a leaf with index `self.start_index`).
    /// Merkle path starts from the bottom of the tree and goes up.
    /// Used to fill in data for trimmed tree leaves when computing Merkle paths and the root hash.
    /// Because only the left subset of the path is used, the cache is not invalidated when new leaves are
    /// pushed into the tree. If all leaves are trimmed, cache is the left subset of the Merkle path to
    /// the next leaf to be inserted, which still has index `self.start_index`.
    cache: Vec<Option<B256>>,
}

impl MiniMerkleTree {
    pub fn new() -> Self {
        let binary_tree_size = 1;
        let depth = Self::tree_depth_by_size(binary_tree_size);

        Self {
            hashes: Default::default(),
            binary_tree_size,
            start_index: 0,
            cache: vec![None; depth],
        }
    }

    pub fn push_hash(&mut self, leaf_hash: B256) {
        self.hashes.push_back(leaf_hash);
        if self.start_index + self.hashes.len() > self.binary_tree_size {
            self.binary_tree_size *= 2;
            if self.cache.len() < Self::tree_depth_by_size(self.binary_tree_size) {
                self.cache.push(None);
            }
        }
    }
    fn tree_depth_by_size(tree_size: usize) -> usize {
        debug_assert!(tree_size.is_power_of_two());
        tree_size.trailing_zeros() as usize
    }

    fn compute_merkle_root_and_path(
        &self,
        mut index: usize,
        mut path: Option<&mut Vec<Option<B256>>>,
        side: Option<Side>,
    ) -> B256 {
        let depth = Self::tree_depth_by_size(self.binary_tree_size);
        if let Some(path) = path.as_deref_mut() {
            path.reserve(depth);
        }

        let mut hashes = self.hashes.clone();
        let mut absolute_start_index = self.start_index;

        for level in 0..depth {
            // If the first untrimmed leaf is a right sibling,
            // add it's left sibling to `hashes` from cache for convenient iteration later.
            if absolute_start_index % 2 == 1 {
                hashes.push_front(self.cache[level].expect("cache is invalid"));
                index += 1;
            }
            // At this point `hashes` always starts from the left sibling node.
            // If it ends on the left sibling node, add the right sibling node to `hashes`
            // for convenient iteration later.
            if hashes.len() % 2 == 1 {
                hashes.push_back(empty_subtree_hash(level));
            }
            if let Some(path) = path.as_deref_mut() {
                let hash = match side {
                    Some(Side::Left) if index % 2 == 0 => None,
                    Some(Side::Right) if index % 2 == 1 => None,
                    _ => hashes.get(index ^ 1).copied(),
                };
                path.push(hash);
            }

            let level_len = hashes.len() / 2;
            // Since `hashes` has an even number of elements, we can simply iterate over the pairs.
            for i in 0..level_len {
                hashes[i] = compress(&hashes[2 * i], &hashes[2 * i + 1]);
            }

            hashes.truncate(level_len);
            index /= 2;
            absolute_start_index /= 2;
        }

        hashes[0]
    }
}

#[derive(Debug, Clone, Copy)]
enum Side {
    Left,
    Right,
}

fn hash_bytes(value: &[u8]) -> B256 {
    B256::from_slice(&keccak256(value))
}

fn compress(lhs: &B256, rhs: &B256) -> B256 {
    let mut bytes = [0_u8; 64];
    bytes[..32].copy_from_slice(&lhs.0);
    bytes[32..].copy_from_slice(&rhs.0);
    B256::from_slice(&keccak256(&bytes))
}

pub fn keccak256(bytes: &[u8]) -> [u8; 32] {
    use tiny_keccak::{Hasher, Keccak};

    let mut output = [0u8; 32];
    let mut hasher = Keccak::v256();
    hasher.update(bytes);
    hasher.finalize(&mut output);
    output
}

fn compute_empty_tree_hashes(empty_leaf_hash: B256) -> Vec<B256> {
    std::iter::successors(Some(empty_leaf_hash), |hash| Some(compress(hash, hash)))
        .take(30 + 1)
        .collect()
}

fn empty_subtree_hash(depth: usize) -> B256 {
    // We do not cache by default since then the cached values would be preserved
    // for all implementations which is not correct for different leaves.
    compute_empty_tree_hashes(empty_leaf_hash())[depth]
}

// TODO: Check if not 88
fn empty_leaf_hash() -> B256 {
    hash_bytes(&[])
}
