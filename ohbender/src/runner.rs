use std::error::Error;

use alloy::primitives::{Address, U256};
use alloy::{hex::FromHex, providers::ProviderBuilder, sol};
use bellman::{bn256::Bn256, plonk::better_better_cs::proof::Proof as PlonkProof};
use circuit_definitions::circuit_definitions::aux_layer::ZkSyncSnarkWrapperCircuit;
use cli::prover_utils::create_final_proofs_from_program_proof;
use zkos_wrapper::{prove_fri_risc_wrapper, prove_risc_wrapper_with_snark};

use crate::batches::{
    call_prove_batches_shared_bridge, create_ohbender_proof_payload,
    fetch_commit_hashes_up_to_batch,
};
use crate::fri::{fetch_fri_proofs, merge_fris};
use std::fs;

pub async fn run_ohbender(
    binary: String,
    output: String,
    trusted_setup_file: Option<String>,
    l1_rpc: String,
    sequencer_rpc: String,
    sequencer_prover_api: String,
) -> Result<(), Box<dyn Error>> {
    // First - let's call the sequencer_rpc 'zks_getBridgehubContract' JSON RPC to get the bridgehub address.

    let bridgehub = get_bridgehub(&sequencer_rpc).await?;
    let chain_id = get_chain_id(&sequencer_rpc).await?;
    let diamond_proxy = get_diamond_proxy(&l1_rpc, &bridgehub, chain_id).await?;

    println!("Bridgehub address: {}", bridgehub);
    println!("Chain ID: {}", chain_id);
    println!("Diamond Proxy address: {}", diamond_proxy);

    let batches_info = get_batches_info(&l1_rpc, &diamond_proxy).await?;
    println!("Batch info: {:?}", batches_info);

    if batches_info.total_batches_verified < batches_info.total_batches_committed {
        println!(
            "Starting to create proof from {} to {} ",
            batches_info.total_batches_verified, batches_info.total_batches_committed
        );
        let start_batch = batches_info.total_batches_verified + 1;
        let end_batch = batches_info.total_batches_committed;

        let provider = ProviderBuilder::new().connect(&l1_rpc).await.unwrap();

        let batches =
            fetch_commit_hashes_up_to_batch(&provider, diamond_proxy, start_batch - 1).await?;

        let proofs = fetch_fri_proofs(sequencer_prover_api, start_batch, end_batch).await?;
        println!("Fetched {} proofs - now merging", proofs.len());
        let merged_proof = merge_fris(proofs, None)?;
        println!("FRI Merge finished - starting final proof");

        fs::write(
            format!("{}/merged.json", output),
            serde_json::to_string_pretty(&merged_proof)?,
        )?;

        let final_proof = create_final_proofs_from_program_proof(merged_proof);
        println!("Final proof ready - starting SNARK wrapping");

        fs::write(
            format!("{}/final.json", output),
            serde_json::to_string_pretty(&final_proof)?,
        )?;

        // This is the workaround for the fact that wrapper and execution utils might be different.
        let serialized_final_proof = serde_json::to_string(&final_proof)?;
        println!("Serialized final proof: {}", serialized_final_proof);
        let wrapper_final_proof: zkos_wrapper::ProgramProof =
            serde_json::from_str(&serialized_final_proof)?;

        let (risc_wrapper, risc_wrapper_vk) =
            prove_fri_risc_wrapper(wrapper_final_proof, Some(binary))?;
        let (snark_proof, snark_vk) =
            prove_risc_wrapper_with_snark(risc_wrapper, risc_wrapper_vk, trusted_setup_file)?;

        // Now we'll ship this snark proof to l1.
        let serialized_snark_proof = serde_json::to_string(&snark_proof)?;

        fs::write(
            format!("{}/snark.json", output),
            serde_json::to_string_pretty(&snark_proof)?,
        )?;
        fs::write(
            format!("{}/snark.vk.json", output),
            serde_json::to_string_pretty(&snark_vk)?,
        )?;

        let codegen_snark_proof: PlonkProof<Bn256, ZkSyncSnarkWrapperCircuit> =
            serde_json::from_str(&serialized_snark_proof)?;
        let (_, serialized_proof) = crypto_codegen::serialize_proof(&codegen_snark_proof);

        let serialized_proof_strings = serialized_proof
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<String>>();

        let proof_payload = create_ohbender_proof_payload(
            &batches,
            serialized_proof_strings,
            start_batch,
            end_batch,
        );

        let _ = call_prove_batches_shared_bridge(
            &provider,
            diamond_proxy,
            start_batch,
            end_batch,
            proof_payload,
        )
        .await;

        return Ok(());
    }

    Ok(())
}

sol! {
    #[sol(rpc)]
    contract IBridgehub {
        function getZKChain(uint256 _chainId) external view returns (address);
    }
    #[sol(rpc)]
    contract IHyperchain {
        function getTotalBatchesCommitted() external view returns (uint256);
        function getTotalBatchesVerified() external view returns (uint256);
        function getTotalBatchesExecuted() external view returns (uint256);
    }
}

pub async fn get_bridgehub(sequencer_rpc: &String) -> Result<String, Box<dyn Error>> {
    let client = reqwest::Client::new();
    let request_body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "zks_getBridgehubContract",
        "params": [],
        "id": 1,
    });

    let response = client
        .post(sequencer_rpc)
        .json(&request_body)
        .send()
        .await?
        .error_for_status()?;

    let response_json: serde_json::Value = response.json().await?;

    Ok(response_json["result"]
        .as_str()
        .ok_or("Failed to parse bridgehub address")?
        .to_string())
}

pub async fn get_chain_id(sequencer_rpc: &String) -> Result<u64, Box<dyn Error>> {
    let client = reqwest::Client::new();
    let request_body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "eth_chainId",
        "params": [],
        "id": 1,
    });

    let response = client
        .post(sequencer_rpc)
        .json(&request_body)
        .send()
        .await?
        .error_for_status()?;

    let response_json: serde_json::Value = response.json().await?;

    let chain_id_hex = response_json["result"]
        .as_str()
        .ok_or("Failed to parse chain id")?;
    let chain_id = u64::from_str_radix(chain_id_hex.trim_start_matches("0x"), 16)?;
    Ok(chain_id)
}

pub async fn get_diamond_proxy(
    l1_rpc: &String,
    bridgehub: &String,
    chain_id: u64,
) -> Result<Address, Box<dyn Error>> {
    let provider = ProviderBuilder::new().connect(l1_rpc).await.unwrap();

    let address = Address::from_hex(bridgehub).unwrap();

    let bridgehub = IBridgehub::new(address, provider.clone());

    let diamond_proxy = bridgehub.getZKChain(U256::from(chain_id)).call().await?;
    println!("Diamond Proxy address: {}", diamond_proxy);

    Ok(diamond_proxy)
}

#[derive(Debug)]
pub struct BatchInfo {
    pub total_batches_committed: u64,
    pub total_batches_verified: u64,
    pub total_batches_executed: u64,
}

pub async fn get_batches_info(
    l1_rpc: &String,
    diamond_proxy: &Address,
) -> Result<BatchInfo, Box<dyn Error>> {
    let provider = ProviderBuilder::new().connect(l1_rpc).await.unwrap();

    let hyperchain = IHyperchain::new(*diamond_proxy, provider.clone());

    let total_batches_committed = hyperchain.getTotalBatchesCommitted().call().await?;
    let total_batches_verified = hyperchain.getTotalBatchesVerified().call().await?;
    let total_batches_executed = hyperchain.getTotalBatchesExecuted().call().await?;

    Ok(BatchInfo {
        total_batches_committed: total_batches_committed.try_into().unwrap(),
        total_batches_verified: total_batches_verified.try_into().unwrap(),
        total_batches_executed: total_batches_executed.try_into().unwrap(),
    })
}
