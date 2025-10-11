use alloy::{
    consensus::Transaction,
    primitives::{Address, B256, Keccak256, U256},
    sol_types::SolCall,
};
use anyhow::Result;
use blake2::{Blake2s256, Digest};
use zk_os_basic_system::system_implementation::flat_storage_model::{
    AccountProperties, VersioningData,
};

use crate::{
    bytecodes::BytecodeInfo,
    contracts::{
        __decodeParamsCall, BlockCommit, CommitBatchInfoZKsyncOS, StoredBatchInfo,
        commitBatchesSharedBridgeCall,
    },
};

/// Struct describing a single batch.
pub struct BatchInfo {
    pub batch_number: u64,
    pub batch_hash: B256,
    pub commitment: B256,
    pub calldata: Vec<u8>,
    pub stored: StoredBatchInfo,
    pub commits: Vec<CommitBatchInfoZKsyncOS>,
    // Block hash, state diffs, logs
    pub blocks_data: Vec<BlockInfo>,
}

impl BatchInfo {
    pub fn parse(
        log: alloy::primitives::Log<BlockCommit>,
        tx: alloy::rpc::types::Transaction,
    ) -> Self {
        let batch = log.batchNumber;
        let batch_hash = log.batchHash;
        let commitment = log.commitment;

        let calldata = tx.input().clone();

        let commit_call = commitBatchesSharedBridgeCall::abi_decode(&calldata).unwrap();

        let commit_data_without_prefix = &commit_call._commitData[1..]; // skip the first byte (version)

        let decode_params = __decodeParamsCall::abi_decode_raw(commit_data_without_prefix).unwrap();

        let (stored, commits) = (decode_params._0, decode_params._1);

        // Currently we only support 1 batch per transaction.
        assert_eq!(
            1,
            commits.len(),
            "Multiple batches per transaction not supported: found {} batches",
            commits.len()
        );

        let blocks_data = parse_da_input(&commits[0].operatorDAInput).unwrap();

        let batch_number: u64 = batch.try_into().unwrap();

        Self {
            batch_number,
            batch_hash,
            commitment,
            calldata: calldata.to_vec(),
            stored,
            commits,
            blocks_data,
        }
    }
}

/// Struct descring a single block.
pub struct BlockInfo {
    pub block_hash: B256,
    pub state_diffs: Vec<StateDiff>,
    pub logs: Vec<Log>,
}

impl BlockInfo {
    /// Reads a single block from a byte stream.
    pub fn parse_from_stream(input: &[u8]) -> Result<(usize, Self)> {
        let pubdata = input;
        tracing::debug!("pubdata len: {}", pubdata.len());

        // now for pubdata itself.

        // First 32 should be some hash.
        if pubdata.len() < 32 {
            tracing::error!("pubdata too short for hash: {}", pubdata.len());
            return Err(anyhow::anyhow!("pubdata too short for hash"));
        }
        // This is the 'current_block_hash' from io_subsystem.rs 'finish'
        let block_header_hash = B256::from_slice(&pubdata[0..32]);
        tracing::debug!("block header hash: 0x{}", hex::encode(block_header_hash));

        let (state_diff_offset, state_diff) = StateDiff::new_from_stream(&pubdata[32..]);

        let remaining = &pubdata[32 + state_diff_offset as usize..];

        // u32 for logs length
        if remaining.len() < 4 {
            tracing::error!("pubdata too short for logs len: {}", remaining.len());
            return Err(anyhow::anyhow!("pubdata too short for logs len"));
        }
        let logs_len = u32::from_be_bytes(
            remaining[0..4]
                .try_into()
                .expect("slice with incorrect length"),
        );
        tracing::debug!("pubdata logs len: {}", logs_len);

        let mut offset = 4;

        let mut logs = Vec::with_capacity(logs_len as usize);

        for _ in 0..logs_len {
            let (consumed, log) = Log::new_from_stream(&remaining[offset..]);

            logs.push(log);
            offset += consumed as usize;
        }

        let messages_len: u32 = if remaining.len() < offset + 4 {
            tracing::error!("pubdata too short for messages len: {}", remaining.len());
            return Err(anyhow::anyhow!("pubdata too short for messages len"));
        } else {
            u32::from_be_bytes(
                remaining[offset..offset + 4]
                    .try_into()
                    .expect("slice with incorrect length"),
            )
        };
        offset += 4;

        tracing::debug!("pubdata messages len: {}", messages_len);

        if messages_len > 0 {
            for _ in 0..messages_len {
                let len = u32::from_be_bytes(
                    remaining[offset..offset + 4]
                        .try_into()
                        .expect("slice with incorrect length"),
                );
                offset += 4;
                tracing::trace!("message len: {}", len);
                offset += len as usize;
            }
        }

        tracing::debug!("pubdata remaining len: {}", remaining.len() - offset);

        Ok((
            offset + 32 + state_diff_offset as usize,
            Self {
                block_hash: block_header_hash,
                state_diffs: state_diff,
                logs,
            },
        ))
    }
}

#[derive(Debug)]
pub struct StateDiff {
    pub derived_key: B256,
    pub value: StateDiffValue,
}

#[derive(Debug)]
pub enum StateDiffValue {
    Value(ValueDiff),
    AccountProperties(AccountPropertiesDiff),
}

#[derive(Debug)]
pub struct AccountPropertiesDiff {
    pub nonce: Option<ValueDiff>,
    pub balance: Option<ValueDiff>,

    // Stuff from full data
    pub versioning: Option<u64>,
    pub bytecode_hash: Option<B256>,
    pub unpadded_code_len: Option<u32>,

    pub artifacts_len: Option<u32>,
    pub observable_bytecode_hash: Option<B256>,
    pub observable_bytecode_len: Option<u32>,
}

#[derive(Debug)]
pub enum ValueDiff {
    Nothing(U256),
    Add(U256),
    Sub(U256),
    Transform(U256),
}

#[derive(Debug)]
pub struct Log {
    pub shard_id: u8,
    pub is_service: bool,
    pub tx_number_in_block: u16,
    pub sender: Address,
    pub key: B256,
    pub value: B256,
}

impl StateDiff {
    /// Parses a list of state diffs from a byte stream.
    pub fn new_from_stream(stream: &[u8]) -> (u64, Vec<Self>) {
        // First u32 is the number of diffs.
        let mut cursor = 0;
        let num_diffs = u32::from_be_bytes(stream[cursor..cursor + 4].try_into().unwrap()) as u64;
        tracing::debug!("Num diffs {:?}", num_diffs);
        cursor += 4;
        let mut diffs = Vec::with_capacity(num_diffs as usize);
        for _ in 0..num_diffs {
            // Each diff starts with 32 bytes of key.
            let derived_key = B256::from_slice(&stream[cursor..cursor + 32]);
            cursor += 32;
            let (consumed, value) = StateDiffValue::new_from_stream(&stream[cursor..]);
            cursor += consumed as usize;
            diffs.push(StateDiff { derived_key, value });
        }

        (cursor as u64, diffs)
    }
}

impl StateDiffValue {
    pub fn new_from_stream(stream: &[u8]) -> (u64, Self) {
        let account_properties_type = (stream[0] & (1 << 2)) != 0;

        if account_properties_type {
            let (offset, diff) = AccountPropertiesDiff::new_from_stream(stream);

            (offset, Self::AccountProperties(diff))
        } else {
            let (offset, value) = ValueDiff::new_from_stream(stream);
            (offset, Self::Value(value))
        }
    }
}

pub fn keccak_digest(data: &[u8]) -> B256 {
    let mut keccak = Keccak256::new();
    keccak.update(data);
    keccak.finalize()
}

impl AccountPropertiesDiff {
    pub fn new_from_stream(stream: &[u8]) -> (u64, Self) {
        let mut cursor = 0;
        let mut nonce = None;
        let mut balance = None;
        let mut versioning = None;
        let mut bytecode_hash = None;
        let mut unpadded_code_len = None;
        let mut artifacts_len = None;
        let mut observable_bytecode_hash = None;
        let mut observable_bytecode_len = None;

        let mut bytecode = None;

        // First byte is a bitmask of which properties are present.
        let bitmask = stream[cursor];
        cursor += 1;

        let account_properties_bit = bitmask & (1 << 2);
        assert!(
            account_properties_bit != 0,
            "account properties bit not set"
        );

        let nonce_update_bit = (bitmask & (1 << 3)) != 0;
        let balance_update_bit = (bitmask & (1 << 4)) != 0;
        let not_publish_bytecode_bit = (bitmask & (1 << 5)) != 0;

        if nonce_update_bit || balance_update_bit {
            // short diff

            if nonce_update_bit {
                let (consumed, diff) = ValueDiff::new_from_stream(&stream[cursor..]);
                cursor += consumed as usize;
                nonce = Some(diff);
            }
            if balance_update_bit {
                let (consumed, diff) = ValueDiff::new_from_stream(&stream[cursor..]);
                cursor += consumed as usize;
                balance = Some(diff);
            }
        } else {
            // full diff
            //println!("Public bytecode bit: {}", not_publish_bytecode_bit);
            // next u64 is versioning data
            versioning = Some(u64::from_be_bytes(
                stream[cursor..cursor + 8].try_into().unwrap(),
            ));
            cursor += 8;

            //println!("Versioning data: {}", _versioning);
            // read nonce and balance
            let (consumed, local_nonce) = ValueDiff::new_from_stream(&stream[cursor..]);
            cursor += consumed as usize;
            let (consumed, local_balance) = ValueDiff::new_from_stream(&stream[cursor..]);
            cursor += consumed as usize;

            nonce = Some(local_nonce);
            balance = Some(local_balance);

            //println!("Full diff nonce: {:#?}", nonce);
            //println!("Full diff balance: {:#?}", balance);

            if not_publish_bytecode_bit {
                bytecode_hash = Some(B256::from_slice(&stream[cursor..cursor + 32]));
                cursor += 32;
                //println!("Bytecode hash: {:?}", bytecode_hash);
            } else {
                // u32 with unpadded code len
                unpadded_code_len = Some(u32::from_be_bytes(
                    stream[cursor..cursor + 4].try_into().unwrap(),
                ));
                cursor += 4;

                artifacts_len = Some(u32::from_be_bytes(
                    stream[cursor..cursor + 4].try_into().unwrap(),
                ));
                cursor += 4;

                let full_bytecode_len = unpadded_code_len.unwrap() as usize
                    + bytecode_padding_len(unpadded_code_len.unwrap() as usize)
                    + artifacts_len.unwrap() as usize;
                bytecode = Some(&stream[cursor..cursor + full_bytecode_len]);
                cursor += full_bytecode_len;

                bytecode_hash = Some(B256::from_slice(&Blake2s256::digest(&bytecode.unwrap())));
            }

            observable_bytecode_len = Some(u32::from_be_bytes(
                stream[cursor..cursor + 4]
                    .try_into()
                    .expect("slice with incorrect length"),
            ));
            cursor += 4;

            if let Some(bytecode) = bytecode {
                let observable_len = observable_bytecode_len.unwrap() as usize;
                let observable_bytecode = &bytecode[..observable_len];
                observable_bytecode_hash = Some(keccak_digest(observable_bytecode));
            }
        }

        (
            cursor as u64,
            Self {
                nonce,
                balance,
                versioning,
                bytecode_hash,
                unpadded_code_len,
                artifacts_len,
                observable_bytecode_hash,

                observable_bytecode_len,
            },
        )
    }

    pub fn update_itself(
        &self,
        old: AccountProperties,
        force_bytecode_info: Option<&&BytecodeInfo>,
    ) -> AccountProperties {
        let mut result = old.clone();
        if let Some(nonce_diff) = &self.nonce {
            let nonce_before = U256::from(result.nonce);

            let nonce_after = match nonce_diff {
                ValueDiff::Nothing(v) => *v,
                ValueDiff::Add(v) => nonce_before.saturating_add(*v),
                ValueDiff::Sub(v) => nonce_before.saturating_sub(*v),
                ValueDiff::Transform(v) => *v,
            };
            result.nonce = nonce_after.try_into().unwrap();
        }
        if let Some(balance_diff) = &self.balance {
            result.balance = match balance_diff {
                ValueDiff::Nothing(v) => *v,
                ValueDiff::Add(v) => result.balance.saturating_add(*v),
                ValueDiff::Sub(v) => result.balance.saturating_sub(*v),
                ValueDiff::Transform(v) => *v,
            };
        }
        if let Some(versioning) = self.versioning {
            result.versioning_data = VersioningData::from_u64(versioning);
        }
        if let Some(bytecode_hash) = self.bytecode_hash {
            result
                .bytecode_hash
                .as_u8_array_mut()
                .copy_from_slice(&bytecode_hash.0);
        }
        if let Some(unpadded_code_len) = self.unpadded_code_len {
            result.unpadded_code_len = unpadded_code_len;
        } else {
            if let Some(force_bytecode_info) = force_bytecode_info {
                result.unpadded_code_len = force_bytecode_info.len.try_into().unwrap();
            }
        }
        if let Some(artifacts_len) = self.artifacts_len {
            result.artifacts_len = artifacts_len;
        } else {
            if let Some(force_bytecode_info) = force_bytecode_info {
                result.artifacts_len = force_bytecode_info.artifacts_len.try_into().unwrap();
            }
        }
        if let Some(observable_bytecode_hash) = self.observable_bytecode_hash {
            result
                .observable_bytecode_hash
                .as_u8_array_mut()
                .copy_from_slice(&observable_bytecode_hash.0);
        } else {
            if let Some(force_bytecode_info) = force_bytecode_info {
                result
                    .observable_bytecode_hash
                    .as_u8_array_mut()
                    .copy_from_slice(&force_bytecode_info.observable_hash.0);
            }
        }
        if let Some(observable_bytecode_len) = self.observable_bytecode_len {
            result.observable_bytecode_len = observable_bytecode_len;
        }
        result
    }
}
pub const fn bytecode_padding_len(deployed_len: usize) -> usize {
    let word = 8;
    let rem = deployed_len % word;
    if rem == 0 { 0 } else { word - rem }
}

impl ValueDiff {
    pub fn new_from_stream(stream: &[u8]) -> (u64, Self) {
        let mut cursor = 0;
        let mode = stream[cursor] & 0b111;

        let length = if mode == 0 { 32 } else { stream[cursor] >> 3 };
        cursor += 1;
        let mut bytes = [0u8; 32];
        if length > 0 {
            bytes[32 - length as usize..]
                .copy_from_slice(&stream[cursor..cursor + length as usize]);
            cursor += length as usize;
        }
        let value = U256::from_be_bytes(bytes);
        let diff = match mode {
            0 => Self::Nothing(value),
            1 => Self::Add(value),
            2 => Self::Sub(value),
            3 => Self::Transform(value),
            _ => panic!("invalid mode"),
        };
        (cursor as u64, diff)
    }
}

// Tests
#[cfg(test)]
mod tests {
    use alloy::hex;

    use super::*;

    #[test]
    fn test_state_diff_parsing() {
        let input = hex!(
            "00000002003ac1e7247f50b6ea3ed2d1c63ce2511668e0d06882fa7a44bbcb0fb31c2e2e1c09010a6431f21254a08b1b0060d9dc74ad5f1ff038e24a6ebc26260a4a03a8d036011b591409640000000000000000"
        );
        let (_, diffs) = StateDiff::new_from_stream(&input);
        assert_eq!(diffs.len(), 2);
    }
}

impl Log {
    pub fn new_from_stream(stream: &[u8]) -> (u64, Self) {
        let mut cursor = 0;
        let shard_id = stream[cursor];
        cursor += 1;
        let is_service = (stream[cursor] & 0b1) != 0;
        cursor += 1;
        let tx_number_in_block = u16::from_be_bytes(stream[cursor..cursor + 2].try_into().unwrap());
        cursor += 2;
        let sender = Address::from_slice(&stream[cursor..cursor + 20]);
        cursor += 20;
        let key = B256::from_slice(&stream[cursor..cursor + 32]);
        cursor += 32;
        let value = B256::from_slice(&stream[cursor..cursor + 32]);
        cursor += 32;

        (
            cursor as u64,
            Self {
                shard_id,
                is_service,
                tx_number_in_block,
                sender,
                key,
                value,
            },
        )
    }
}

/// Reads DA input (assuming that it uses calldata, not blobs)
/// and returns block info information.
pub fn parse_da_input(input: &[u8]) -> Result<Vec<BlockInfo>> {
    // first 32 bytes should be 0, the next 32 is some keccak.
    if input.len() < 64 {
        tracing::error!("DA input too short: {}", input.len());
        return Err(anyhow::anyhow!("DA input too short"));
    }
    // not sure what this prefix is..
    let prefix = &input[0..32];
    if prefix.iter().any(|&b| b != 0) {
        tracing::error!("DA input prefix not zero: {:x?}", prefix);
        return Err(anyhow::anyhow!("DA input prefix not zero"));
    }

    let pubdata_hash = &input[32..64];
    tracing::debug!("pubdata input hash: 0x{}", hex::encode(pubdata_hash));
    let blob_count = &input[64];
    // for calldata, blobcount should be 1.
    tracing::debug!("blob count: {}", blob_count);

    let mut offset = 65;
    // another 32 bytes that should be 0.
    if input.len() < offset + 32 {
        tracing::error!("DA input too short for second zero: {}", input.len());
        return Err(anyhow::anyhow!("DA input too short for second zero"));
    }
    let mid = &input[offset..offset + 32];
    if mid.iter().any(|&b| b != 0) {
        tracing::error!("DA input mid not zero: {:x?}", mid);
        return Err(anyhow::anyhow!("DA input mid not zero"));
    }
    offset += 32;

    let calldata_type = &input[offset];
    tracing::debug!("calldata type: {}", calldata_type);
    if *calldata_type != 0 {
        return Err(anyhow::anyhow!(
            "Unsupported calldata type: {} (only type 0 is supported)",
            calldata_type
        ));
    }

    offset += 1;

    // remaining bytes:

    let mut results = vec![];
    loop {
        tracing::debug!("parsing block {}", results.len());
        let (consumed, block_info) = BlockInfo::parse_from_stream(&input[offset..])?;
        tracing::debug!(
            "offset: {} consumed: {} input len: {}",
            offset,
            consumed,
            input.len()
        );
        offset += consumed;
        results.push(block_info);
        // Last 32 bytes should be a blob commitment.
        if offset + 32 >= input.len() {
            break;
        }
    }
    let blob_commitment = B256::from_slice(&input[offset..offset + 32]);
    tracing::debug!("blob commitment: {:?}", blob_commitment);
    if blob_commitment != B256::ZERO {
        return Err(anyhow::anyhow!(
            "Expected zero blob commitment, got: {:?}",
            blob_commitment
        ));
    }

    Ok(results)
}
