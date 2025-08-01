// SPDX-License-Identifier: MIT

pragma solidity 0.8.28;

struct PriorityOperation {
    bytes32 canonicalTxHash;
    uint64 expirationTimestamp;
    uint192 layer2Tip;
}
struct Queue {
    mapping(uint256 priorityOpId => PriorityOperation priorityOp) data;
    uint256 tail;
    uint256 head;
}

struct Bytes32PushTree {
    uint256 _nextLeafIndex;
    bytes32[] _sides;
    bytes32[] _zeros;
}

struct Tree {
    uint256 startIndex; // priority tree started accepting priority ops from this index
    uint256 unprocessedIndex; // relative to `startIndex`
    mapping(bytes32 => bool) historicalRoots;
    Bytes32PushTree tree;
}

/// @notice Indicates whether an upgrade is initiated and if yes what type
/// @param None Upgrade is NOT initiated
/// @param Transparent Fully transparent upgrade is initiated, upgrade data is publicly known
/// @param Shadow Shadow upgrade is initiated, upgrade data is hidden
enum UpgradeState {
    None,
    Transparent,
    Shadow
}

/// @dev Logically separated part of the storage structure, which is responsible for everything related to proxy
/// upgrades and diamond cuts
/// @param proposedUpgradeHash The hash of the current upgrade proposal, zero if there is no active proposal
/// @param state Indicates whether an upgrade is initiated and if yes what type
/// @param securityCouncil Address which has the permission to approve instant upgrades (expected to be a Gnosis
/// multisig)
/// @param approvedBySecurityCouncil Indicates whether the security council has approved the upgrade
/// @param proposedUpgradeTimestamp The timestamp when the upgrade was proposed, zero if there are no active proposals
/// @param currentProposalId The serial number of proposed upgrades, increments when proposing a new one
struct UpgradeStorage {
    bytes32 proposedUpgradeHash;
    UpgradeState state;
    address securityCouncil;
    bool approvedBySecurityCouncil;
    uint40 proposedUpgradeTimestamp;
    uint40 currentProposalId;
}

/// @notice The struct that describes whether users will be charged for pubdata for L1->L2 transactions.
/// @param Rollup The users are charged for pubdata & it is priced based on the gas price on Ethereum.
/// @param Validium The pubdata is considered free with regard to the L1 gas price.
enum PubdataPricingMode {
    Rollup,
    Validium
}

/// @notice The fee params for L1->L2 transactions for the network.
/// @param pubdataPricingMode How the users will charged for pubdata in L1->L2 transactions.
/// @param batchOverheadL1Gas The amount of L1 gas required to process the batch (except for the calldata).
/// @param maxPubdataPerBatch The maximal number of pubdata that can be emitted per batch.
/// @param priorityTxMaxPubdata The maximal amount of pubdata a priority transaction is allowed to publish.
/// It can be slightly less than maxPubdataPerBatch in order to have some margin for the bootloader execution.
/// @param minimalL2GasPrice The minimal L2 gas price to be used by L1->L2 transactions. It should represent
/// the price that a single unit of compute costs.
struct FeeParams {
    PubdataPricingMode pubdataPricingMode;
    uint32 batchOverheadL1Gas;
    uint32 maxPubdataPerBatch;
    uint32 maxL2GasPerBatch;
    uint32 priorityTxMaxPubdata;
    uint64 minimalL2GasPrice;
}

struct VerifierParams {
    bytes32 recursionNodeLevelVkHash;
    bytes32 recursionLeafLevelVkHash;
    bytes32 recursionCircuitsSetVksHash;
}

struct ZKChainStorage {
    /// @dev Storage of variables needed for deprecated diamond cut facet
    uint256[7] __DEPRECATED_diamondCutStorage;
    /// @notice Address which will exercise critical changes to the Diamond Proxy (upgrades, freezing & unfreezing). Replaced by CTM
    address __DEPRECATED_governor;
    /// @notice Address that the governor proposed as one that will replace it
    address __DEPRECATED_pendingGovernor;
    /// @notice List of permitted validators
    mapping(address validatorAddress => bool isValidator) validators;
    /// @dev Verifier contract. Used to verify aggregated proof for batches
    address verifier;
    /// @notice Total number of executed batches i.e. batches[totalBatchesExecuted] points at the latest executed batch
    /// (batch 0 is genesis)
    uint256 totalBatchesExecuted;
    /// @notice Total number of proved batches i.e. batches[totalBatchesProved] points at the latest proved batch
    uint256 totalBatchesVerified;
    /// @notice Total number of committed batches i.e. batches[totalBatchesCommitted] points at the latest committed
    /// batch
    uint256 totalBatchesCommitted;
    /// @dev Stored hashed StoredBatch for batch number
    mapping(uint256 batchNumber => bytes32 batchHash) storedBatchHashes;
    /// @dev Stored root hashes of L2 -> L1 logs
    mapping(uint256 batchNumber => bytes32 l2LogsRootHash) l2LogsRootHashes;
    /// @dev Container that stores transactions requested from L1
    Queue priorityQueue;
    /// @dev The smart contract that manages the list with permission to call contract functions
    address __DEPRECATED_allowList;
    VerifierParams __DEPRECATED_verifierParams;
    /// @notice Bytecode hash of bootloader program.
    /// @dev Used as an input to zkp-circuit.
    bytes32 l2BootloaderBytecodeHash;
    /// @notice Bytecode hash of default account (bytecode for EOA).
    /// @dev Used as an input to zkp-circuit.
    bytes32 l2DefaultAccountBytecodeHash;
    /// @dev Indicates that the porter may be touched on L2 transactions.
    /// @dev Used as an input to zkp-circuit.
    bool zkPorterIsAvailable;
    /// @dev The maximum number of the L2 gas that a user can request for L1 -> L2 transactions
    /// @dev This is the maximum number of L2 gas that is available for the "body" of the transaction, i.e.
    /// without overhead for proving the batch.
    uint256 priorityTxMaxGasLimit;
    /// @dev Storage of variables needed for upgrade facet
    UpgradeStorage __DEPRECATED_upgrades;
    /// @dev A mapping L2 batch number => message number => flag.
    /// @dev The L2 -> L1 log is sent for every withdrawal, so this mapping is serving as
    /// a flag to indicate that the message was already processed.
    /// @dev Used to indicate that eth withdrawal was already processed
    mapping(uint256 l2BatchNumber => mapping(uint256 l2ToL1MessageNumber => bool isFinalized)) isEthWithdrawalFinalized;
    /// @dev The most recent withdrawal time and amount reset
    uint256 __DEPRECATED_lastWithdrawalLimitReset;
    /// @dev The accumulated withdrawn amount during the withdrawal limit window
    uint256 __DEPRECATED_withdrawnAmountInWindow;
    /// @dev A mapping user address => the total deposited amount by the user
    mapping(address => uint256) __DEPRECATED_totalDepositedAmountPerUser;
    /// @dev Stores the protocol version. Note, that the protocol version may not only encompass changes to the
    /// smart contracts, but also to the node behavior.
    uint256 protocolVersion;
    /// @dev Hash of the system contract upgrade transaction. If 0, then no upgrade transaction needs to be done.
    bytes32 l2SystemContractsUpgradeTxHash;
    /// @dev Batch number where the upgrade transaction has happened. If 0, then no upgrade transaction has happened
    /// yet.
    uint256 l2SystemContractsUpgradeBatchNumber;
    /// @dev Address which will exercise non-critical changes to the Diamond Proxy (changing validator set & unfreezing)
    address admin;
    /// @notice Address that the admin proposed as one that will replace admin role
    address pendingAdmin;
    /// @dev Fee params used to derive gasPrice for the L1->L2 transactions. For L2 transactions,
    /// the bootloader gives enough freedom to the operator.
    /// @dev The value is only for the L1 deployment of the ZK Chain, since payment for all the priority transactions is
    /// charged at that level.
    FeeParams feeParams;
    /// @dev Address of the blob versioned hash getter smart contract used for EIP-4844 versioned hashes.
    /// @dev Used only for testing.
    address blobVersionedHashRetriever;
    /// @dev The chainId of the chain
    uint256 chainId;
    /// @dev The address of the bridgehub
    address bridgehub;
    /// @dev The address of the ChainTypeManager
    address chainTypeManager;
    /// @dev The address of the baseToken contract. Eth is address(1)
    address __DEPRECATED_baseToken;
    /// @dev The address of the baseTokenbridge. Eth also uses the shared bridge
    address __DEPRECATED_baseTokenBridge;
    /// @notice gasPriceMultiplier for each baseToken, so that each L1->L2 transaction pays for its transaction on the destination
    /// we multiply by the nominator, and divide by the denominator
    uint128 baseTokenGasPriceMultiplierNominator;
    uint128 baseTokenGasPriceMultiplierDenominator;
    /// @dev The optional address of the contract that has to be used for transaction filtering/whitelisting
    address transactionFilterer;
    /// @dev The address of the l1DAValidator contract.
    /// This contract is responsible for the verification of the correctness of the DA on L1.
    address l1DAValidator;
    /// @dev The address of the contract on L2 that is responsible for the data availability verification.
    /// This contract sends `l2DAValidatorOutputHash` to L1 via L2->L1 system log and it will routed to the `l1DAValidator` contract.
    address l2DAValidator;
    /// @dev the Asset Id of the baseToken
    bytes32 baseTokenAssetId;
    /// @dev If this ZKchain settles on this chain, then this is zero. Otherwise it is the address of the ZKchain that is a
    /// settlement layer for this ZKchain. (think about it as a 'forwarding' address for the chain that migrated away).
    address settlementLayer;
    /// @dev Priority tree, the new data structure for priority queue
    Tree priorityTree;
    /// @dev Whether the chain is a permanent rollup. Note, that it only enforces the DA validator pair, but
    /// it does not enforce any other parameters, e.g. `pubdataPricingMode`
    bool isPermanentRollup;
    /// @notice Bytecode hash of evm emulator.
    /// @dev Used as an input to zkp-circuit.
    bytes32 l2EvmEmulatorBytecodeHash;
    /// @dev Boojum OS flag, if `true` settlement done using Boojum OS state transition, otherwise Era VM
    bool boojumOS;
}

contract SetVerifierHelper {
    ZKChainStorage internal s;

    function setVerifier(address _verifier) external {
        s.verifier = _verifier;
    }
}
