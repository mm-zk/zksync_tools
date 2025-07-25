use std::collections::HashMap;

use alloy::{hex::FromHex, primitives::B256, providers::Provider, sol_types::SolCall};
use reqwest::Client;
use serde_json::Value;

use crate::{
    IHyperchain::{self, IHyperchainInstance},
    StoredBatchInfo,
    l1_merkle::MerkleInfoForExecute,
};

pub async fn get_l1_tx_for_block(l2_sequencer: &str, block: u64) -> Vec<String> {
    let client = Client::new();

    // First, get block hash from block number.
    let block_number_hex = format!("0x{:x}", block);

    let transaction_hashes = {
        let req_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_getBlockByNumber",
            "params": [block_number_hex, false]
        });
        let resp = client
            .post(l2_sequencer)
            .json(&req_body)
            .send()
            .await
            .expect("Failed to send request");
        let json: Value = resp.json().await.expect("Invalid JSON");

        json["result"]["transactions"]
            .as_array()
            .expect("Transactions not found")
            .iter()
            .map(|x| x.as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };

    let transactions = {
        let req_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_getBlockByNumber",
            "params": [block_number_hex, true]
        });
        let resp = client
            .post(l2_sequencer)
            .json(&req_body)
            .send()
            .await
            .expect("Failed to send request");
        let json: Value = resp.json().await.expect("Invalid JSON");

        json["result"]["transactions"]
            .as_array()
            .expect("Transactions not found")
            .clone()
    };

    assert_eq!(transaction_hashes.len(), transactions.len());

    let l1_hashes = transaction_hashes
        .iter()
        .zip(transactions.iter())
        .filter_map(|(tx_hash, tx)| {
            if tx["type"].as_str() == Some("0x2a") {
                Some(tx_hash.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    l1_hashes
}

pub async fn execute_batches<P: Provider + Clone>(
    contract: IHyperchainInstance<P>,
    start: u64,
    end: u64,
    l2_sequencer: &str,
    stored: &HashMap<u64, StoredBatchInfo>,
    dry_run: bool,
) {
    // Execute start
    let mut execute_data = vec![0u8];

    let new_batches = (start..=end)
        .map(|x| stored.get(&x).unwrap().clone())
        .collect();

    let mut l1_tx_map = HashMap::new();

    // Actually start from block 1.
    for block in 1..=end {
        let l1_txs = get_l1_tx_for_block(l2_sequencer, block.into()).await;
        let txs = l1_txs
            .iter()
            .map(|tx_hash| B256::from_hex(tx_hash).expect("Invalid L1 transaction hash"))
            .collect::<Vec<B256>>();
        l1_tx_map.insert(block, txs);
    }

    let merkle_info = MerkleInfoForExecute::init(&l1_tx_map);

    let priority_ops = (start..=end)
        .map(|x| {
            let batch = stored.get(&x).unwrap();
            let batch_l1_txs: u64 = batch.numberOfLayer1Txs.try_into().unwrap();
            println!("Batch {} l1txs: {}", x, batch.numberOfLayer1Txs);
            println!("priority op hash: {}", batch.priorityOperationsHash);
            let item_hashes = l1_tx_map.get(&x).unwrap().clone();
            assert_eq!(item_hashes.len() as u64, batch_l1_txs);

            let (root, left_path, right_path) =
                merkle_info.get_merkle_path_for_l1_tx_in_block(x).clone();
            println!("Merkle root: {}", root);
            println!("Left path: {:?}", left_path);
            println!("Right path: {:?}", right_path);

            // Number of item hashes must match number of l1tx in a given batch.
            //let item_hashes = vec![FixedBytes::ZERO; batch.numberOfLayer1Txs.try_into().unwrap()];
            IHyperchain::PriorityOpsBatchInfo {
                leftPath: left_path,
                rightPath: right_path,
                itemHashes: item_hashes,
            }
        })
        .collect();

    let proof_payload = IHyperchain::executePayloadCall {
        executeData: new_batches,
        priorityOps: priority_ops,
    };

    proof_payload.abi_encode_raw(&mut execute_data);

    let _ = contract
        .executeBatchesSharedBridge(
            0.try_into().unwrap(),
            start.try_into().unwrap(),
            end.try_into().unwrap(),
            execute_data.clone().into(),
        )
        .call()
        .await
        .unwrap();

    if dry_run == false {
        let tx = contract
            .executeBatchesSharedBridge(
                0.try_into().unwrap(),
                start.try_into().unwrap(),
                end.try_into().unwrap(),
                execute_data.into(),
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
        println!("\x1b[32mExecute call was successful.\x1b[0m");
        println!("\x1b[31mThis was just a dry-run\x1b[0m");
    }
}
