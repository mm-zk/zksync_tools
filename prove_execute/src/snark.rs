use base64;
use bellman::{bn256::Bn256, plonk::better_better_cs::proof::Proof as PlonkProof};
use circuit_definitions::circuit_definitions::aux_layer::ZkSyncSnarkWrapperCircuit;

use serde_json::Value;
use std::error::Error;
use std::fs;

pub fn parse_snark(path: &str) -> Result<Vec<String>, Box<dyn Error>> {
    // Load the JSON file from disk.
    let file_content = fs::read_to_string(path)?;
    let json_value: Value = serde_json::from_str(&file_content)?;

    let inner_value = if json_value.get("proof").is_some() {
        // Extract the "result" field (a base64-encoded JSON string).
        let encoded = json_value
            .get("proof")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'proof' field or it isn't a string")?;

        // Decode the base64 string.
        let decoded_bytes = base64::decode(encoded)?;

        // Parse the decoded string as JSON.
        let inner_value: PlonkProof<Bn256, ZkSyncSnarkWrapperCircuit> =
            bincode::deserialize(&decoded_bytes)?;
        inner_value
    } else {
        let inner_value: PlonkProof<Bn256, ZkSyncSnarkWrapperCircuit> =
            serde_json::from_str(&file_content)?;
        inner_value
    };

    println!("Inner Value: {:?}", inner_value);

    let (inputs, serialized_proof) = crypto_codegen::serialize_proof(&inner_value);

    println!("A: {:?}", inputs);
    println!("B: {:?}", serialized_proof);

    Ok(serialized_proof.iter().map(|x| x.to_string()).collect())
}
