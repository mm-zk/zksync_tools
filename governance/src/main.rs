use alloy::{
    primitives::{Address, B256, FixedBytes, keccak256},
    signers::local::PrivateKeySigner,
};
use clap::Parser;

use alloy::{hex::FromHex, providers::ProviderBuilder, sol};

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
        event BlockCommit(uint256 indexed batchNumber, bytes32 indexed batchHash, bytes32 indexed commitment);

        function getChainTypeManager() external view returns (address);

    }

    enum Action {
        Add,
        Replace,
        Remove
    }

    struct FacetCut {
        address facet;
        Action action;
        bool isFreezable;
        bytes4[] selectors;
    }

    /// @dev Structure of the diamond proxy changes
    /// @param facetCuts The set of changes (adding/removing/replacement) of implementation contracts
    /// @param initAddress The address that's delegate called after setting up new facet changes
    /// @param initCalldata Calldata for the delegate call to `initAddress`
    struct DiamondCutData {
        FacetCut[] facetCuts;
        address initAddress;
        bytes initCalldata;
    }

    #[sol(rpc)]
    contract ChainTypeManager {
        address public admin;

        address public owner;

        function executeUpgrade(uint256 _chainId, DiamondCutData calldata _diamondCut) external;

    }

    struct Call {
        address target;
        uint256 value;
        bytes data;
    }


    struct Operation {
        Call[] calls;
        bytes32 predecessor;
        bytes32 salt;
    }

    #[sol(rpc)]
    contract Governance {
        address public owner;
        function scheduleTransparent(Operation calldata _operation, uint256 _delay) external;
        function execute(Operation calldata _operation) external;
    }

    #[sol(rpc)]
    contract ChainAdminOwner {
        address public owner;
        function multicall(Call[] calldata _calls, bool _requireSuccess) external payable;
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

pub fn get_batch_public_input(prev_batch: &StoredBatchInfo, batch: &StoredBatchInfo) -> B256 {
    let mut bytes = Vec::with_capacity(32 * 3);
    bytes.extend_from_slice(prev_batch.batchHash.as_slice());
    bytes.extend_from_slice(batch.batchHash.as_slice());
    bytes.extend_from_slice(batch.commitment.as_slice());
    keccak256(&bytes).into()
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    address: String,
    #[arg(short, long)]
    server_url: Option<String>,

    #[arg(long)]
    chain_id: u64,

    #[arg(long)]
    method_name: String,

    #[arg(long)]
    new_address: String,

    #[arg(long)]
    governance_private_key: String,
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();
    let server = args
        .server_url
        .unwrap_or_else(|| "http://localhost:8545".to_string());
    println!("Diamond Proxy: {}", args.address);

    let signer: PrivateKeySigner = args.governance_private_key.parse().unwrap();
    let signer_address = signer.address();

    let provider = ProviderBuilder::new()
        .wallet(signer)
        .connect(&server)
        .await
        .unwrap();

    let address = Address::from_hex(args.address).unwrap();

    let contract = IHyperchain::new(address, provider.clone());

    println!("Verifier: {}", contract.getVerifier().call().await.unwrap());

    let method_selector = keccak256(args.method_name.as_bytes());
    println!(
        "Computed signature: 0x{}",
        hex::encode(&method_selector[..4])
    );

    let chain_type_manager = contract.getChainTypeManager().call().await.unwrap();
    println!("ChainTypeManager: {}", chain_type_manager);

    let chain_type_manager_contract = ChainTypeManager::new(chain_type_manager, provider.clone());
    let governance = chain_type_manager_contract.owner().call().await.unwrap();
    println!("ChainTypeManager governance: {}", governance);

    let governance_contract = Governance::new(governance, provider.clone());
    let owner = governance_contract.owner().call().await.unwrap();
    println!("Governance owner: {}", owner);

    if signer_address != owner {
        panic!(
            "Governance private key does not match the owner address: {}. Look for the key in wallets.yaml for the chain or ecosystem.",
            signer_address
        );
    }

    let execute_ctm_upgrade = chain_type_manager_contract.executeUpgrade(
        args.chain_id.try_into().unwrap(),
        DiamondCutData {
            facetCuts: vec![FacetCut {
                facet: Address::from_hex(args.new_address).unwrap(),
                action: Action::Add,
                isFreezable: false,
                selectors: vec![method_selector[..4].try_into().unwrap()],
            }],
            initAddress: Address::ZERO,
            initCalldata: Default::default(),
        },
    );

    let operation = Operation {
        calls: vec![Call {
            target: chain_type_manager,
            value: Default::default(),
            data: execute_ctm_upgrade.calldata().clone(),
        }],
        predecessor: FixedBytes::ZERO,
        salt: FixedBytes::ZERO,
    };

    if false {
        let tx = governance_contract
            .scheduleTransparent(operation.clone(), 0u64.try_into().unwrap())
            .send()
            .await
            .unwrap();

        println!("Schedule Transaction sent: {}", tx.tx_hash());
        let receipt = tx.get_receipt().await.unwrap();
        if receipt.status() {
            println!("Transaction succeeded: {:?}", receipt);
        } else {
            println!("Transaction failed: {:?}", receipt);
        }
    }
    {
        let tx = governance_contract.execute(operation).send().await.unwrap();

        println!("Schedule Transaction sent: {}", tx.tx_hash());
        let receipt = tx.get_receipt().await.unwrap();
        if receipt.status() {
            println!("Transaction succeeded: {:?}", receipt);
        } else {
            println!("Transaction failed: {:?}", receipt);
        }
    }
}
