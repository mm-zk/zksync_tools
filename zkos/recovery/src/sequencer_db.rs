use std::path::PathBuf;

use alloy::rlp::Encodable;
use alloy::{consensus::Block, primitives::B256};
use anyhow::{Context, Result};
use rocksdb::DB;

use crate::state::{BlockchainState, hash_leaf};
use bincode::{Decode, Encode};

// Data from priority tree that we cache in the database.
#[derive(Debug, Encode, Decode)]
pub struct CachedTreeData {
    /// Index of the leftmost untrimmed leaf.
    pub start_index: usize,
    /// Left subset of the Merkle path to the first untrimmed leaf
    #[bincode(with_serde)]
    pub cache: Vec<Option<B256>>,
}

pub fn write_to_db(db_path: &String, blockchain_state: BlockchainState) -> Result<()> {
    let wal_path = PathBuf::from(db_path).join("block_replay_wal");

    let mut wal_db = rocksdb::DB::open_default(wal_path)
        .with_context(|| format!("open RocksDB at {}", db_path))?;

    // create column family in wal_db
    wal_db
        .create_cf("latest", &rocksdb::Options::default())
        .with_context(|| "create column family 'latest' in wal_db")?;

    let state_path = PathBuf::from(db_path).join("state");

    let mut state_db = rocksdb::DB::open_default(state_path)
        .with_context(|| format!("open RocksDB at {}", db_path))?;

    let preimages_path = PathBuf::from(db_path).join("preimages");
    let mut preimages_db = rocksdb::DB::open_default(preimages_path)
        .with_context(|| format!("open RocksDB at {}", db_path))?;

    for cf_name in ["meta", "storage"] {
        preimages_db
            .create_cf(cf_name, &rocksdb::Options::default())
            .with_context(|| format!("create column family '{cf_name}' in preimages_db"))?;
    }

    preimages_db.put_cf(
        preimages_db.cf_handle("meta").unwrap(),
        b"block_number",
        &blockchain_state.current_block.to_be_bytes(),
    )?;

    for (k, v) in blockchain_state.preimage_store.iter() {
        preimages_db.put_cf(
            preimages_db.cf_handle("storage").unwrap(),
            k.as_slice(),
            v.as_slice(),
        )?;
    }

    let tree_path = PathBuf::from(db_path).join("tree");
    let mut tree_db = rocksdb::DB::open_default(tree_path)
        .with_context(|| format!("open RocksDB at {}", db_path))?;

    for cf_name in ["key_indices"] {
        tree_db
            .create_cf(cf_name, &rocksdb::Options::default())
            .with_context(|| format!("create column family '{cf_name}' in tree_db"))?;
    }

    init_tree_in_rocksdb(&blockchain_state, &mut tree_db).unwrap();

    let repository_path = PathBuf::from(db_path).join("repository");
    let mut repository_db = rocksdb::DB::open_default(repository_path)
        .with_context(|| format!("open RocksDB at {}", db_path))?;

    for cf_name in [
        "block_data",
        "block_number_to_hash",
        "tx",
        "tx_receipt",
        "tx_meta",
        "initiator_and_nonce_to_hash",
        "meta",
    ] {
        repository_db
            .create_cf(cf_name, &rocksdb::Options::default())
            .with_context(|| format!("create column family '{cf_name}' in repository_db"))?;
    }

    let repository_meta_cf = repository_db.cf_handle("meta").unwrap();
    repository_db.put_cf(
        repository_meta_cf,
        b"block_number",
        &blockchain_state.current_block.to_be_bytes(),
    )?;

    // We also (seems that) we have to add stuff for 'block 0' (this is hardcoded in some places).
    // Not sure if this is really needed, but doesn't hurt.

    let zero_block_number = 0u64;
    let block_number_to_hash_cf = repository_db.cf_handle("block_number_to_hash").unwrap();
    repository_db.put_cf(
        block_number_to_hash_cf,
        &zero_block_number.to_be_bytes(),
        &blockchain_state.genesis_state.header.hash_slow().0,
    )?;

    // then we also have to put block data.
    let block_data_cf = repository_db.cf_handle("block_data").unwrap();
    let block: Block<B256> = Block {
        header: blockchain_state.genesis_state.header.clone(),
        body: Default::default(),
    };

    let mut block_bytes = Vec::new();
    block.encode(&mut block_bytes);

    repository_db.put_cf(block_data_cf, &block.header.hash_slow().0, &block_bytes)?;

    let priority_txs_path = PathBuf::from(db_path).join("priority_txs_tree");
    let mut priority_txs_db = rocksdb::DB::open_default(priority_txs_path)
        .with_context(|| format!("open RocksDB at {}", db_path))?;

    priority_txs_db
        .create_cf("cached_tree_data", &rocksdb::Options::default())
        .with_context(|| "create column family 'cached_tree_data' in priority_txs_db")?;

    let priority_txs_cf = priority_txs_db.cf_handle("cached_tree_data").unwrap();

    priority_txs_db.put_cf(
        priority_txs_cf,
        b"block_number",
        &blockchain_state.current_block.to_be_bytes(),
    )?;

    let destination = &mut [0u8; 1000];

    let result = bincode::encode_into_slice(
        &CachedTreeData {
            start_index: 0,
            cache: vec![None; 32],
        },
        &mut destination[..],
        bincode::config::standard(),
    )?;

    // TODO: put some real data here.
    priority_txs_db.put_cf(priority_txs_cf, b"data", &destination[..result])?;

    state_db
        .create_cf("meta", &rocksdb::Options::default())
        .with_context(|| "create column family 'meta' in state_db")?;
    state_db
        .create_cf("storage", &rocksdb::Options::default())
        .with_context(|| "create column family 'storage' in state_db")?;

    let state_meta_cf = state_db.cf_handle("meta").unwrap();
    let state_storage_cf = state_db.cf_handle("storage").unwrap();

    state_db
        .put_cf(
            state_meta_cf,
            b"base_block",
            blockchain_state.current_block.to_be_bytes(),
        )
        .with_context(|| "write base block to state_db")?;

    // insert all the states.
    for leaf in blockchain_state.tree.leaves.iter() {
        let k = leaf.key;
        let v = leaf.value;
        state_db
            .put_cf(state_storage_cf, k.as_slice(), v.as_slice())
            .with_context(|| "write storage log to state_db")?;
    }

    let latest_cf = wal_db.cf_handle("latest").unwrap();

    //  insert latest block into column family 'latest' in wal_db
    wal_db
        .put_cf(
            latest_cf,
            b"latest_block",
            blockchain_state.current_block.to_be_bytes(),
        )
        .with_context(|| "write latest block to wal_db")?;

    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct Manifest {
    /// Number of tree versions stored in the database.
    pub(crate) version_count: u64,
    pub(crate) tags: TreeTags,
}

impl Manifest {
    pub fn serialize(manifest: &Manifest) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        leb128::write::unsigned(&mut buffer, manifest.version_count).unwrap();
        manifest.tags.serialize(&mut buffer);
        Ok(buffer)
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct TreeTags {
    pub architecture: String,
    pub depth: u8,
    pub internal_node_depth: u8,
    pub hasher: String,
}

impl Default for TreeTags {
    fn default() -> Self {
        Self {
            architecture: "AmortizedLinkedListMT".into(),
            depth: 64,
            internal_node_depth: 3,
            hasher: "Blake2s256".into(),
        }
    }
}

impl TreeTags {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        let entry_count = 4; // custom tags aren't supported (yet?)
        leb128::write::unsigned(buffer, entry_count).unwrap();

        Self::serialize_str(buffer, "architecture");
        Self::serialize_str(buffer, &self.architecture);
        Self::serialize_str(buffer, "depth");
        Self::serialize_str(buffer, &self.depth.to_string());
        Self::serialize_str(buffer, "internal_node_depth");
        Self::serialize_str(buffer, &self.internal_node_depth.to_string());
        Self::serialize_str(buffer, "hasher");
        Self::serialize_str(buffer, &self.hasher);
    }
    fn serialize_str(bytes: &mut Vec<u8>, s: &str) {
        leb128::write::unsigned(bytes, s.len() as u64).unwrap();
        bytes.extend_from_slice(s.as_bytes());
    }
}

pub const LEAF_NIBBLES: u8 = 22; // div_ceil(64 / 3)

fn init_tree_in_rocksdb(blockchain_state: &BlockchainState, tree_db: &mut DB) -> Result<()> {
    // First - put manifest.
    let manifest = Manifest {
        version_count: blockchain_state.current_block + 1, // TODO: explain why +1.
        tags: TreeTags::default(),
    };

    println!(
        "Writing manifest with version_count: {}",
        blockchain_state.current_block
    );

    tree_db.put(&[0], Manifest::serialize(&manifest)?)?;

    // We'll assume that all the entries are coming from this block.
    let entries_version = blockchain_state.current_block;

    /*for leaf in blockchain_state.tree.leaves.iter() {
        if leaf.next_index >= blockchain_state.tree.leaves.len() as u64 {
            anyhow::bail!(
                "Leaf with key {:?} has invalid next_index {}",
                leaf.key,
                leaf.next_index
            );
        }
        let next = leaf.next_index;
        let next_key = blockchain_state.tree.leaves[next as usize].key;
        assert!(
            next_key > leaf.key,
            "Leaves are not sorted {} -> {}",
            leaf.key,
            next_key
        );
    }*/

    // Then - put all leaves.
    for (i, original_leaf) in blockchain_state.tree.leaves.iter().enumerate() {
        let node_key = NodeKey {
            version: entries_version,
            nibble_count: LEAF_NIBBLES,
            index_on_level: i as u64,
        };
        let key = node_key.as_db_key();
        let mut buffer = vec![];

        // TODO: cleanup.
        let local_leaf = Leaf {
            key: original_leaf.key,
            value: original_leaf.value,
            next_index: original_leaf.next_index,
        };

        local_leaf.serialize(&mut buffer);

        tree_db.put(key, &buffer).unwrap();
    }

    let mut hashes = blockchain_state
        .tree
        .leaves
        .iter()
        .map(|x| hash_leaf(x))
        .collect::<Vec<_>>();

    let mut current_zero = hash_leaf(&crate::state::Leaf::default());

    let mut level = 0;

    for nibble_level in (0..LEAF_NIBBLES).rev() {
        let mut new_hashes = vec![];

        // level 0 is special now.
        let (level_multiplier, level_multiplier_log) =
            if nibble_level == 0 { (2, 1) } else { (8, 3) };

        let nodes_at_level = hashes.len().div_ceil(level_multiplier); //1_u64 << (nibble_level * 3);
        tracing::debug!(
            "Writing level {}, which has {} nodes",
            nibble_level,
            nodes_at_level
        );
        let mut nodes = vec![];
        for index_on_level in 0..nodes_at_level {
            let node_key = NodeKey {
                version: entries_version,
                nibble_count: nibble_level,
                index_on_level: index_on_level as u64,
            };
            let key = node_key.as_db_key();
            let mut buffer = vec![];

            let mut children = vec![];
            let first_child_index = index_on_level * level_multiplier;
            for i in 0..level_multiplier {
                let child_index = first_child_index + i;
                if child_index < hashes.len() {
                    children.push(ChildRef {
                        version: entries_version,
                        hash: hashes[child_index],
                    });
                }
            }
            let internal_node = InternalNode { children };
            internal_node.serialize(&mut buffer);
            tree_db.put(key, &buffer).unwrap();
            nodes.push(internal_node.clone());

            // now let's compute the hash of this internal node.
            let mut sub_hashes = vec![current_zero; level_multiplier];
            for i in 0..internal_node.children.len() {
                // if we have a child, use its hash.
                let child = &internal_node.children[i];
                sub_hashes[i] = child.hash;
            }

            for sub_level in 0..level_multiplier_log {
                let limit = level_multiplier >> sub_level;
                for i in 0..limit / 2 {
                    sub_hashes[i] =
                        crate::state::compress(&sub_hashes[i * 2], &sub_hashes[i * 2 + 1]);
                }
            }
            new_hashes.push(sub_hashes[0]);
        }

        for _ in 0..level_multiplier_log {
            current_zero = crate::state::compress(&current_zero, &current_zero);
            level = level + 1;
        }

        if nibble_level == 0 {
            let root = Root {
                leaf_count: blockchain_state.tree.leaves.len() as u64,
                root_node: nodes[0].clone(),
            };
            let node_key = NodeKey::root(entries_version);
            let mut destination = vec![];
            root.serialize(&mut destination);

            tree_db.put(&node_key.as_db_key(), &destination).unwrap();
        }

        hashes = new_hashes;
    }

    tracing::info!(
        "Tree initialized, root hash: {:?} leaf count: {}",
        hashes[0],
        blockchain_state.tree.leaves.len()
    );

    // Small sanity check.
    assert_eq!(hashes[0], blockchain_state.tree.compute_root());

    // And now indices.

    let indices_cf = tree_db.cf_handle("key_indices").unwrap();

    for (i, leaf) in blockchain_state.tree.leaves.iter().enumerate() {
        let key = leaf.key;

        let inserted_entry = InsertedKeyEntry {
            index: i as u64,
            inserted_at: entries_version,
        };
        let mut data = vec![];
        inserted_entry.serialize(&mut data);

        tree_db.put_cf(indices_cf, key.as_slice(), &data)?;
    }

    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeKey {
    /// Tree version.
    pub(crate) version: u64,
    /// 0 is root, 1 is its children etc.
    /// and nibble level 22 are leaves.
    pub(crate) nibble_count: u8,
    pub(crate) index_on_level: u64,
}

impl NodeKey {
    pub(crate) const fn root(version: u64) -> Self {
        Self {
            version,
            nibble_count: 0,
            index_on_level: 0,
        }
    }
    const DB_KEY_LEN: usize = 8 + 1 + 8;

    fn as_db_key(&self) -> [u8; Self::DB_KEY_LEN] {
        let mut buffer = [0_u8; Self::DB_KEY_LEN];
        buffer[..8].copy_from_slice(&self.version.to_be_bytes());
        buffer[8] = self.nibble_count;
        buffer[9..].copy_from_slice(&self.index_on_level.to_be_bytes());
        buffer
    }
}

#[derive(Debug, Clone)]
pub struct Root {
    pub(crate) leaf_count: u64,
    pub(crate) root_node: InternalNode,
}
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub struct InternalNode {
    pub(crate) children: Vec<ChildRef>,
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct ChildRef {
    pub(crate) version: u64,
    pub(crate) hash: B256,
}

impl Root {
    pub(super) fn serialize(&self, buffer: &mut Vec<u8>) {
        leb128::write::unsigned(buffer, self.leaf_count).unwrap();
        self.root_node.serialize(buffer);
    }
}
impl InternalNode {
    pub(super) fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer.push(self.children.len() as u8);

        for child_ref in &self.children {
            child_ref.serialize(buffer);
        }
    }
}

impl ChildRef {
    fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(self.hash.as_slice());
        leb128::write::unsigned(buffer, self.version).unwrap();
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub enum Node {
    Internal(InternalNode),
    Leaf(Leaf),
}

/// Tree leaf.
#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Leaf {
    pub key: B256,
    pub value: B256,
    /// 0-based index of a leaf with the lexicographically next key.
    pub next_index: u64,
}

impl Leaf {
    pub(super) fn serialize(&self, buffer: &mut Vec<u8>) {
        buffer.extend_from_slice(self.key.as_slice());
        buffer.extend_from_slice(self.value.as_slice());
        leb128::write::unsigned(buffer, self.next_index).unwrap();
    }
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(test, derive(PartialEq))]
struct InsertedKeyEntry {
    index: u64,
    inserted_at: u64,
}

impl InsertedKeyEntry {
    pub(super) fn serialize(&self, buffer: &mut Vec<u8>) {
        leb128::write::unsigned(buffer, self.index).unwrap();
        leb128::write::unsigned(buffer, self.inserted_at).unwrap();
    }
}
