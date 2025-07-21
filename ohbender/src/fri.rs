use std::{error::Error, fs};

use base64::Engine;
use cli::{
    Machine,
    prover_utils::{
        GpuSharedState, ProofMetadata, VerifierCircuitsIdentifiers, create_proofs_internal,
        generate_oracle_data_from_metadata_and_proof_list,
        program_proof_from_proof_list_and_metadata, proof_list_and_metadata_from_program_proof,
    },
};
use execution_utils::{ProgramProof, UNIVERSAL_CIRCUIT_VERIFIER, get_padded_binary};
use serde_json::Value;
use std::io::Write;

pub async fn fetch_fri_proof(
    sequencer_prover_api: String,
    batch_id: u64,
) -> Result<ProgramProof, Box<dyn Error>> {
    let url = format!("{}/prover-jobs/FRI/{}", sequencer_prover_api, batch_id);
    let client = reqwest::Client::new();

    let response = client.get(&url).send().await?.error_for_status()?;

    if response.status().is_success() {
        let content = response.text().await?;

        load_fri_from_from_string(&content)
    } else {
        Err(Box::from("Failed to fetch FRI proof"))
    }
}

pub async fn fetch_fri_proofs(
    sequencer_prover_api: String,
    start_batch_id: u64,
    end_batch_id: u64,
) -> Result<Vec<ProgramProof>, Box<dyn Error>> {
    let mut proofs = Vec::new();
    for batch_id in start_batch_id..=end_batch_id {
        match fetch_fri_proof(sequencer_prover_api.clone(), batch_id).await {
            Ok(proof) => proofs.push(proof),
            Err(e) => println!("Failed to fetch proof for batch {}: {}", batch_id, e),
        }
    }
    Ok(proofs)
}

pub fn load_fri_from_file(path: &str) -> Result<ProgramProof, Box<dyn Error>> {
    // Load the JSON file from disk.
    let file_content = fs::read_to_string(path)?;

    load_fri_from_from_string(&file_content)
}

fn load_fri_from_from_string(content: &String) -> Result<ProgramProof, Box<dyn Error>> {
    let json_value: Value = serde_json::from_str(content)?;
    if json_value.get("proof").is_some() {
        // Extract the "result" field (a base64-encoded JSON string).
        let encoded = json_value
            .get("proof")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'result' field or it isn't a string")?;

        // Decode the base64 string.
        let decoded_bytes = base64::engine::general_purpose::STANDARD.decode(encoded)?;
        let inner_value: ProgramProof = bincode::deserialize(&decoded_bytes)?;
        Ok(inner_value)
    } else {
        let inner_value: ProgramProof = serde_json::from_str(content)?;
        Ok(inner_value)
    }
}

pub fn merge_fris(
    proofs: Vec<ProgramProof>,
    tmp_dir: Option<String>,
) -> Result<ProgramProof, Box<dyn Error>> {
    if proofs.is_empty() {
        return Err(Box::from("No proofs to merge"));
    }
    let (metadata, _) = proof_list_and_metadata_from_program_proof(proofs[0].clone());

    let mut gpu_shared_state = GpuSharedState::new(&get_padded_binary(UNIVERSAL_CIRCUIT_VERIFIER));

    let mut result = proofs.first().ok_or("No proofs provided")?.clone();
    for (id, next) in proofs.iter().skip(1).enumerate() {
        println!("Merging proof {} of {}", id + 1, proofs.len());
        let first_oracle = proof_to_recursion_oracle(&result);
        let next_oracle = proof_to_recursion_oracle(next);

        // Merge each oracle with the first one.
        result = merge_two(
            first_oracle,
            next_oracle,
            &metadata,
            &mut Some(&mut gpu_shared_state),
            // &mut None,
        );
        if let Some(tmp_dir) = &tmp_dir {
            let intermediate_output = format!("{}/merged_{}.json", tmp_dir, id + 1);
            let json_output = serde_json::to_string_pretty(&result)?;
            fs::write(&intermediate_output, json_output)?;
            println!("Intermediate merged proof saved to {}", intermediate_output);
        }
    }

    Ok(result)
}

/// Takes a program proof (assumes it is from recursion) - and creates a oracle that can be used as input to verifier.
fn proof_to_recursion_oracle(proof: &ProgramProof) -> Vec<u32> {
    let (metadata, list) = proof_list_and_metadata_from_program_proof(proof.clone());
    generate_oracle_data_from_metadata_and_proof_list(&metadata, &list)
}

fn merge_two(
    first_oracle: Vec<u32>,
    second_oracle: Vec<u32>,
    current_proof_metadata: &ProofMetadata,
    gpu_shared_state: &mut Option<&mut GpuSharedState>,
) -> ProgramProof {
    let mut merged = vec![VerifierCircuitsIdentifiers::CombinedRecursionLayers as u32];

    merged.extend(first_oracle);
    merged.extend(second_oracle);

    //u32_to_file(&"merged".to_string(), &merged);

    let binary = get_padded_binary(UNIVERSAL_CIRCUIT_VERIFIER);
    let mut timing = Some(0f64);
    let (current_proof_list, current_proof_metadata) = create_proofs_internal(
        &binary,
        merged,
        &Machine::Reduced,
        100, // Guessing - FIXME!!
        Some(current_proof_metadata.create_prev_metadata()),
        gpu_shared_state,
        &mut timing,
    );
    program_proof_from_proof_list_and_metadata(&current_proof_list, &current_proof_metadata)
}

#[allow(dead_code)]
fn u32_to_file(output_file: &String, numbers: &[u32]) {
    // Open the file for writing
    let mut file = fs::File::create(output_file).expect("Failed to create file");

    // Write each u32 as an 8-character hexadecimal string without newlines
    for &num in numbers {
        write!(file, "{:08X}", num).expect("Failed to write to file");
    }

    println!("Successfully wrote to file: {}", output_file);
}
