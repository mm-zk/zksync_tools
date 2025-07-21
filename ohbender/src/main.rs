use base64::{self, Engine};
use bellman::{bn256::Bn256, plonk::better_better_cs::proof::Proof as PlonkProof};
use circuit_definitions::circuit_definitions::aux_layer::ZkSyncSnarkWrapperCircuit;

use clap::{Parser, Subcommand};

use serde_json::Value;
use std::error::Error;
use std::fs;

use crate::{
    fri::{load_fri_from_file, merge_fris},
    runner::run_ohbender,
};

mod batches;
mod fri;
mod runner;

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
    Run {
        #[arg(long)]
        binary: String,
        #[arg(long)]
        output: String,
        #[arg(long)]
        trusted_setup_file: Option<String>,
        #[arg(long)]
        l1_rpc: String,
        #[arg(long)]
        sequencer_rpc: String,
        #[arg(long)]
        sequencer_prover_api: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let opts = Cli::parse();
    match opts.command {
        Command::ParseFri { file, output } => parse_fri(&file, output),
        Command::ParseSnark { file } => parse_snark(&file),
        Command::MergeFri {
            files,
            output,
            tmp_dir,
        } => merge_fris_from_files(files, output, tmp_dir),
        Command::Run {
            binary,
            output,
            trusted_setup_file,
            l1_rpc,
            sequencer_rpc,
            sequencer_prover_api,
        } => {
            run_ohbender(
                binary,
                output,
                trusted_setup_file,
                l1_rpc,
                sequencer_rpc,
                sequencer_prover_api,
            )
            .await
        }
    }?;

    Ok(())
}

pub fn merge_fris_from_files(
    files: Vec<String>,
    output_file: String,
    tmp_dir: Option<String>,
) -> Result<(), Box<dyn Error>> {
    let proofs = files
        .into_iter()
        .map(|file| load_fri_from_file(&file))
        .collect::<Result<Vec<_>, _>>()?;

    let result = merge_fris(proofs, tmp_dir)?;

    // Serialize the merged proof as pretty JSON and write it to the output file.
    let json_output = serde_json::to_string_pretty(&result)?;
    fs::write(&output_file, json_output)?;
    println!("Merged proof saved to {}", output_file);

    Ok(())
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

    // VERIFICATION KEY REGISTERS (18-25)
    println!("Verification Key Registers (18-25):");
    for i in 18..=25 {
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
        let decoded_bytes = base64::engine::general_purpose::STANDARD.decode(encoded)?;

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
