use std::collections::HashMap;

use clap::Parser;

use alloy::{
    consensus::Transaction,
    hex::FromHex,
    primitives::{Address, TxHash, U256},
    providers::{Provider, ProviderBuilder},
    sol,
    sol_types::SolCall,
};

mod snark;

sol! {
    #[sol(rpc)]
    contract IHyperchain {
        function getVerifier() external view returns (address);
        function getAdmin() external view returns (address);
        function getTotalBatchesCommitted() external view returns (uint256);
        function getTotalBatchesVerified() external view returns (uint256);
        function getTotalBatchesExecuted() external view returns (uint256);
        function getSemverProtocolVersion() external view returns (uint32, uint32, uint32);

        function getL2BootloaderBytecodeHash() external view returns (bytes32);
        function getL2DefaultAccountBytecodeHash() external view returns (bytes32);
        function getL2SystemContractsUpgradeTxHash() external view returns (bytes32);
        function getChainId() external view returns (uint256);
        function getSettlementLayer() external view returns (address);

        function getPriorityQueueSize() external view returns (uint256);
        function getTotalPriorityTxs() external view returns (uint256);
        function getPriorityTreeRoot() external view returns (bytes32);

        function commitBatchesSharedBridge(uint256,uint256,uint256,bytes);

        function tmpStuff(StoredBatchInfo stored, CommitBoojumOSBatchInfo[] commits) external;

        function proofPayload(StoredBatchInfo old, StoredBatchInfo[] newInfo, uint256[] proof);

        function proveBatchesSharedBridge(
            uint256, // _chainId
            uint256 _processBatchFrom,
            uint256 _processBatchTo,
            bytes calldata _proofData
        );
    }
    #[derive(Debug)]

    struct StoredBatchInfo {
        uint64 batchNumber;
        bytes32 batchHash; // For Boojum OS batches we'll store here full state commitment
        uint64 indexRepeatedStorageChanges; // For Boojum OS not used, 0
        uint256 numberOfLayer1Txs;
        bytes32 priorityOperationsHash;
        bytes32 l2LogsTreeRoot;
        uint256 timestamp; // For Boojum OS not used, 0
        bytes32 commitment;// For Boojum OS batches we'll store batch output hash here
    }
    #[derive(Debug)]

    struct CommitBoojumOSBatchInfo {
        uint64 batchNumber;
        // chain state commitment, this preimage is not opened on l1,
        // it's guaranteed that this commitment commits to any state that needed for execution
        // (state root, block number, bloch hahes)
        bytes32 newStateCommitment;
        // info about processed l1 txs, l2 to l1 logs and DA
        uint256 numberOfLayer1Txs;
        bytes32 priorityOperationsHash;
        bytes32 l2LogsTreeRoot;
        address l2DaValidator; // TODO: already saved in the storage, can just add from there to PI
        bytes32 daCommitment;
        // sending used batch inputs to validate on the settlement layer
        uint64 firstBlockTimestamp;
        uint64 lastBlockTimestamp;
        uint256 chainId; // TODO: already saved in the storage, can just add from there to PI
        // extra calldata to pass to da validator
        bytes operatorDAInput;
    }

    #[derive(Debug)]

    struct Merged {
        StoredBatchInfo foo;
        CommitBoojumOSBatchInfo[] bar;
    }
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    address: String,
    #[arg(short, long)]
    server_url: Option<String>,
}

#[tokio::main]

async fn main() {
    let args = Cli::parse();
    let server = args
        .server_url
        .unwrap_or_else(|| "http://localhost:8545".to_string());
    println!("Diamond Proxy: {}", args.address);
    let provider = ProviderBuilder::new().connect(&server).await.unwrap();

    let address = Address::from_hex(args.address).unwrap();

    let contract = IHyperchain::new(address, provider.clone());

    println!("Verifier: {}", contract.getVerifier().call().await.unwrap());

    let transactions = [
        "0xdba68527f0fa45d1492c491d0ff96f901e2f319f20195218f0541c831a85e7da",
        "0xa941c7fd5ae27c73ac420b19d2d9689d85b4ebf82b58d464eb96d5de41568ebb",
        "0xe47363114780670144e032502a532fe0b9122495e7c5a717ab91625bf9ca629b",
        "0x2ff28754039a48701328d77bb97ed52195eb8aad39711c86238e88bf4519d1e7",
        // skipped - good.
        "0xe672c197695b59cf74ac15bf4b7664a4b1892c38958354f43bb084f3261befb0",
        // skipped - good.
        "0xdddb319eb27f54e258ed4f8bc5d92445f55513c4957c28121d78fd07b2e9c13c",
    ];

    let mut batches = HashMap::new();
    let mut stored = HashMap::new();

    for tx in transactions {
        let tx_hash = TxHash::from_hex(tx).unwrap();
        let tx_data = provider
            .get_transaction_by_hash(tx_hash)
            .await
            .unwrap()
            .expect("Transaction not found");

        let aa = tx_data.inner;
        let bb = aa.as_eip1559().unwrap();
        let cc = bb.tx();

        if cc.input[..4] != IHyperchain::commitBatchesSharedBridgeCall::SELECTOR {
            println!("Skipping transaction: {}", tx);
            continue;
        }

        let decoded = IHyperchain::commitBatchesSharedBridgeCall::abi_decode(cc.input()).unwrap();
        {
            println!("First parameter: {}", decoded._0);
            println!("Second parameter: {}", decoded._1);
            println!("Third parameter: {}", decoded._2);
        }

        let commit_data = &decoded._3.clone()[1..];
        //println!("commit data: 0x{}", hex::encode(commit_data));
        //let ww = IHyperchain::tmpStuffCall::abi_decode_raw(commit_data).unwrap();
        println!("Looking at tx: {}", tx);

        let ww = IHyperchain::tmpStuffCall::abi_decode_raw(commit_data).unwrap();
        for other in ww.commits {
            println!("CommitBoojumOSBatchInfo: {:?}", other.batchNumber);
            batches.insert(other.batchNumber, other);
        }
        stored.insert(ww.stored.batchNumber, ww.stored);
    }
    let data = snark::parse_snark("merged_2.snark").unwrap();
    println!("Parsed SNARK data: {:?}", data[0]);

    println!("data: {:?}", data[0]);

    let mut proof: Vec<U256> = data
        .iter()
        .map(|x| U256::from_str_radix(x, 10).unwrap())
        .collect();

    // ohbender type
    proof.insert(0, U256::from(2));
    // FRI from batch 1.
    proof.insert(
        1,
        U256::from_str_radix(
            "309f3397494dd66536462742c2661015cac60f3efffcbf11c28cdee0691cc6e9",
            16,
        )
        .unwrap(),
    );

    let mut proofData = vec![0u8];

    let from = 2;
    let to = 3;

    let pp = IHyperchain::proofPayloadCall {
        old: stored.get(&1).unwrap().clone(),
        newInfo: vec![
            stored.get(&2).unwrap().clone(),
            stored.get(&3).unwrap().clone(),
        ],
        proof,
    };

    pp.abi_encode_raw(&mut proofData);

    //;::abi_encode_raw(&self, out);

    let result = contract
        .proveBatchesSharedBridge(
            0.try_into().unwrap(),
            2.try_into().unwrap(),
            3.try_into().unwrap(),
            proofData.into(),
        )
        .call()
        .await
        .unwrap();
}
