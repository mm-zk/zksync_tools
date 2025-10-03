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
/// Things related to 'state' genesis (loading basic set of contracts from json file).

#[derive(Debug, Serialize, Deserialize)]
pub struct GenesisInput {
    /// Initial contracts to deploy in genesis.
    /// Storage entries that set the contracts as deployed and preimages will be derived from this field.
    pub initial_contracts: Vec<(Address, alloy::primitives::Bytes)>,
    /// Additional (not related to contract deployments) storage entries to add in genesis state.
    pub additional_storage: Vec<(B256, B256)>,
}

// load string from genesis.json and then parse.

#[derive(Debug, Clone, Serialize, Deserialize)]
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
