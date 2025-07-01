use base64;
use bellman::{
    bn256::Bn256,
    plonk::better_better_cs::proof::{Proof as PlonkProof, Proof},
};
use circuit_definitions::circuit_definitions::aux_layer::ZkSyncSnarkWrapperCircuit;

use clap::{Parser, Subcommand};
use cli::{
    Machine,
    prover_utils::{
        ProofMetadata, VerifierCircuitsIdentifiers, create_proofs_internal,
        create_recursion_proofs, generate_oracle_data_from_metadata_and_proof_list,
        program_proof_from_proof_list_and_metadata, proof_list_and_metadata_from_program_proof,
    },
};
use execution_utils::{ProgramProof, UNIVERSAL_CIRCUIT_VERIFIER, get_padded_binary};
use serde_json::Value;
use std::error::Error;
use std::fs;
use std::io::Write;

#[derive(Debug, Parser)]

struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Displays suggested values to use.
    ParseFri {
        file: String,
        #[arg(long)]
        output: Option<String>,
    },
    ParseSnark {
        file: String,
    },
    MergeFri {
        files: Vec<String>,
        #[arg(long)]
        output: String,
        #[arg(long)]
        tmp_dir: Option<String>,
    },
}

fn main() -> Result<(), Box<dyn Error>> {
    let opts = Cli::parse();
    match opts.command {
        Command::ParseFri { file, output } => parse_fri(&file, output),
        Command::ParseSnark { file } => parse_snark(&file),
        Command::MergeFri {
            files,
            output,
            tmp_dir,
        } => merge_fris(files, output, tmp_dir),
    }?;

    Ok(())
}

pub fn merge_fris(
    files: Vec<String>,
    output_file: String,
    tmp_dir: Option<String>,
) -> Result<(), Box<dyn Error>> {
    let proofs = files
        .into_iter()
        .map(|file| load_fri_from_file(&file))
        .collect::<Result<Vec<_>, _>>()?;

    let (metadata, _) = proof_list_and_metadata_from_program_proof(proofs[0].clone());

    let mut result = proofs.first().ok_or("No proofs provided")?.clone();
    for (id, next) in proofs.iter().skip(1).enumerate() {
        println!("Merging proof {} of {}", id + 1, proofs.len());
        let first_oracle = proof_to_recursion_oracle(&result);
        let next_oracle = proof_to_recursion_oracle(next);
        // Merge each oracle with the first one.
        result = merge_two(first_oracle, next_oracle, &metadata);
        if let Some(tmp_dir) = &tmp_dir {
            let intermediate_output = format!("{}/merged_{}.json", tmp_dir, id + 1);
            let json_output = serde_json::to_string_pretty(&result)?;
            fs::write(&intermediate_output, json_output)?;
            println!("Intermediate merged proof saved to {}", intermediate_output);
        }
    }

    // Serialize the merged proof as pretty JSON and write it to the output file.
    let json_output = serde_json::to_string_pretty(&result)?;
    fs::write(&output_file, json_output)?;
    println!("Merged proof saved to {}", output_file);

    Ok(())
}

fn u32_to_file(output_file: &String, numbers: &[u32]) {
    // Open the file for writing
    let mut file = fs::File::create(output_file).expect("Failed to create file");

    // Write each u32 as an 8-character hexadecimal string without newlines
    for &num in numbers {
        write!(file, "{:08X}", num).expect("Failed to write to file");
    }

    println!("Successfully wrote to file: {}", output_file);
}

fn merge_two(
    first_oracle: Vec<u32>,
    second_oracle: Vec<u32>,
    current_proof_metadata: &ProofMetadata,
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
        10, // Guessing!!
        Some(current_proof_metadata.create_prev_metadata()),
        &mut None, // gpu_shared_state,
        &mut timing,
    );
    program_proof_from_proof_list_and_metadata(&current_proof_list, &current_proof_metadata)
}

/// Takes a program proof (assumes it is from recursion) - and creates a oracle that can be used as input to verifier.
fn proof_to_recursion_oracle(proof: &ProgramProof) -> Vec<u32> {
    let (metadata, list) = proof_list_and_metadata_from_program_proof(proof.clone());
    generate_oracle_data_from_metadata_and_proof_list(&metadata, &list)
}

fn load_fri_from_file(path: &str) -> Result<ProgramProof, Box<dyn Error>> {
    // Load the JSON file from disk.
    let file_content = fs::read_to_string(path)?;
    let json_value: Value = serde_json::from_str(&file_content)?;
    if json_value.get("proof").is_some() {
        // Extract the "result" field (a base64-encoded JSON string).
        let encoded = json_value
            .get("proof")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'result' field or it isn't a string")?;

        // Decode the base64 string.
        let decoded_bytes = base64::decode(encoded)?;
        let inner_value: ProgramProof = bincode::deserialize(&decoded_bytes)?;
        Ok(inner_value)
    } else {
        let inner_value: ProgramProof = serde_json::from_str(&file_content)?;
        Ok(inner_value)
    }
}

pub fn parse_fri(path: &str, output: Option<String>) -> Result<(), Box<dyn Error>> {
    let inner_value = load_fri_from_file(path)?;
    if let Some(output_filename) = &output {
        let json_output = serde_json::to_string_pretty(&inner_value)?;
        fs::write(output_filename, json_output)?;
        println!("Inner value saved to {}", output_filename);
    }

    println!(
        "Register count: {}",
        inner_value.register_final_values.len()
    );

    inner_value.register_final_values.iter().for_each(|v| {
        println!("Register Final Value: {:?}", v.value);
    });

    // now look at registers from 10 - 17 (inclusive).

    for i in 10..=17 {
        if let Some(register) = inner_value.register_final_values.get(i) {
            let hex_value = hex::encode(&register.value.to_le_bytes());
            println!("Register {}: {:?} (hex: {})", i, register.value, hex_value);
        } else {
            println!("Register {}: Not found", i);
        }
    }
    // And now concatenated (without 17)
    let mut concatenated = String::new();
    for i in 10..=16 {
        if let Some(register) = inner_value.register_final_values.get(i) {
            concatenated = concatenated + &hex::encode(&register.value.to_le_bytes());
        }
    }
    println!("'snark': Concatenated registers 10-16: {}", concatenated);
    concatenated = concatenated
        + &hex::encode(
            inner_value
                .register_final_values
                .get(17)
                .unwrap()
                .value
                .to_le_bytes(),
        );
    println!("'fri': Concatenated registers 10-17: {}", concatenated);

    let mut concatenated = String::new();
    for i in 10..=17 {
        if let Some(register) = inner_value.register_final_values.get(i) {
            concatenated = concatenated + &hex::encode(&register.value.to_be_bytes());
        }
    }
    println!("'fri(be)': Concatenated registers 10-17: {}", concatenated);

    Ok(())
}

pub fn parse_snark(path: &str) -> Result<(), Box<dyn Error>> {
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
    Ok(())
}
