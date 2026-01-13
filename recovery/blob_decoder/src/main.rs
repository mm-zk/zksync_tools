use clap::Parser;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use zkevm_circuits::eip_4844::ethereum_4844_data_into_zksync_pubdata;

#[derive(Parser)]
#[command(name = "zkSync Blob Decoder")]
#[command(about = "Decodes EIP-4844 Blobs back into zkSync Pubdata (performs iFFT + BitReverse)", long_about = None)]
struct Cli {
    /// Path to a file containing the Blob Hex string (0x...)
    #[arg(short, long, value_name = "FILE")]
    input: PathBuf,

    /// Output file path for the raw decoded binary
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();

    println!("Reading blob from {:?}...", cli.input);
    let blob_hex_raw = fs::read_to_string(&cli.input).expect("Failed to read input file");
    let blob_hex = blob_hex_raw.trim().trim_start_matches("0x");

    let blob_bytes = hex::decode(blob_hex).expect("Invalid Hex String");

    if blob_bytes.len() != 131072 {
        eprintln!(
            "Error: Invalid Blob size. Expected 131072 bytes, got {}. \
            Ensure you are providing the full 4844 sidecar data.",
            blob_bytes.len()
        );
        return;
    }

    println!("Blob loaded. Performing Inverse FFT and Bit Reversal...");
    let pubdata = ethereum_4844_data_into_zksync_pubdata(&blob_bytes);
    let pubdata_hex = hex::encode(&pubdata);
    
    println!("Successfully decoded! Pubdata Input Size: {} bytes", pubdata.len());
    println!("Pubdata Input: {}", pubdata_hex);

    // Write to file
    if let Some(output) = cli.output {
        let mut file = fs::File::create(&output).expect("Failed to create output file");
        file.write_all(pubdata_hex.as_bytes()).expect("Failed to write to output file");
        println!("Saved pubdata input to {:?}", output);
    }
}
