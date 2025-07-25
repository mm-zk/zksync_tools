use std::collections::HashMap;

use alloy::{primitives::U256, providers::Provider, sol_types::SolCall};

use crate::{
    IHyperchain::{self, IHyperchainInstance},
    StoredBatchInfo, snark, snark_public_input_for_range,
};

const OHBENDER_PROOF_TYPE: i32 = 2;
const FAKE_PROOF_TYPE: i32 = 3;
const FAKE_PROOF_MAGIC_VALUE: i32 = 13;

pub async fn fake_prove_batches<P: Provider + Clone>(
    contract: IHyperchainInstance<P>,
    start: u64,
    end: u64,
    stored: &HashMap<u64, StoredBatchInfo>,
    public_input: String,
    dry_run: bool,
) {
    let public_input = if public_input.starts_with("0x") {
        public_input.trim_start_matches("0x").to_string()
    } else {
        public_input
    };
    let proof: Vec<U256> = vec![
        // Fake proof type
        U256::from(FAKE_PROOF_TYPE),
        // OhBender 'previous hash' - for fake proof, we can always assume that it matches the range perfectly.
        U256::from(0),
        // Fake proof magic value (just for sanity)
        U256::from(FAKE_PROOF_MAGIC_VALUE),
        // Public input (fake proof **will** verify this against batch data stored in the contract)
        U256::from_str_radix(&public_input, 16).unwrap(),
    ];

    prove_batches_internal(proof, contract, start, end, stored, dry_run).await;
}

pub async fn prove_batches<P: Provider + Clone>(
    contract: IHyperchainInstance<P>,
    start: u64,
    end: u64,
    stored: &HashMap<u64, StoredBatchInfo>,
    snark_start: Option<u64>,
    snark_path: String,
    dry_run: bool,
) {
    let data = snark::load_snark_from_file(&snark_path).unwrap();
    let mut proof: Vec<U256> = data
        .iter()
        .map(|x| U256::from_str_radix(x, 10).unwrap())
        .collect();

    // ohbender type
    proof.insert(0, U256::from(OHBENDER_PROOF_TYPE));

    // FRI from batch 1.

    let prev_hash = match snark_start {
        Some(snark_start) => {
            assert!(
                snark_start <= start,
                "Snark start must be less than or equal to start"
            );
            // If snark start is provided, we use it to get the previous batch.
            if snark_start < start {
                // compute keccak256 from snark_start to start-1 inclusive.
                // TODO: check if BE or LE.
                let public_input = snark_public_input_for_range(stored, snark_start, start - 1);
                U256::from_be_slice(public_input.as_slice())
            } else {
                U256::from(0)
            }
        }
        None => {
            // If none - then snark range is matching the range perfectly.
            U256::from(0)
        }
    };
    proof.insert(1, prev_hash);

    prove_batches_internal(proof, contract, start, end, stored, dry_run).await
}

pub async fn prove_batches_internal(
    proof: Vec<U256>,
    contract: IHyperchainInstance<impl Provider>,
    start: u64,
    end: u64,
    stored: &HashMap<u64, StoredBatchInfo>,
    dry_run: bool,
) {
    let prev_batch = start - 1;

    let mut proof_data = vec![0u8];

    let new_batches = (start..=end)
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
            start.try_into().unwrap(),
            end.try_into().unwrap(),
            proof_data.clone().into(),
        )
        .call()
        .await
        .unwrap();

    if dry_run == false {
        let tx = contract
            .proveBatchesSharedBridge(
                0.try_into().unwrap(),
                start.try_into().unwrap(),
                end.try_into().unwrap(),
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
    } else {
        println!("\x1b[32mProve call was successful.\x1b[0m");
        println!("\x1b[31mThis was just a dry-run\x1b[0m");
    }
}
