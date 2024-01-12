# tool to prove that given transaction was included in L2.

# step 1 - check the block hash
# step 2 - fetch the pubdata (to verify the block hashes)

from web3 import Web3
import requests
import json
from eth_abi import decode
from enum import Enum
from pubdata  import parse_pubdata
from Crypto.Hash import keccak
from merkle_proof import get_storage_proof, verify_storage_proof


TRANSACTION_TO_PROVE = "0x849c9f33ecd4fddc6f11a270180e39d99386a6074c23fdfcb7cd6ad9034aa47e"
ZKSYNC_URL = 'https://mainnet.era.zksync.io'
ETH_URL = 'https://rpc.ankr.com/eth'


WHITELISTED_ADDRESSES = set(
    [
        "0x32400084c286cf3e17e7b677ea9583e60a000324", # zksync era mainnet diamond proxy
        "0xa0425d71cB1D6fb80E65a5361a04096E0672De03", # zksync era timelock
    ]
)


# Checks that tx belongs to a block.
# Retuns the block number and block hash and (unverified batch number).
# After calling this - you should verify that this block and 
# hash was correctly included in the chain.
def verify_tx_inclusion_in_block(txn):    
    web3 = Web3(Web3.HTTPProvider(ZKSYNC_URL))
    # Check if connected successfully
    if not web3.is_connected():
        print("Failed to connect to zkSync node.")
        raise 
    
    print(f"\033[92m[OK]\033[0m Connected to {ZKSYNC_URL}")

    # Fetch the transaction
    try:
        tx = web3.eth.get_transaction(txn)
    except Exception as e:
        print(f"An error occurred: {e}")
        raise 
    
    print(f"\033[92m[OK]\033[0m Transaction {txn} found. Checking block {tx['blockNumber']}")
    

    # now fetch the blockinfo
    try:
        block = web3.eth.get_block(tx['blockNumber'])
    except Exception as e:
        print(f"An error occurred: {e}")
        raise
    
    print(f"\033[92m[OK]\033[0m Block found with hash {block['hash'].hex()}.")
    
    transactions_in_block = block['transactions']
    
    found = False
    for transaction in transactions_in_block:
        if transaction.hex() == txn:
            found = True

    if not found:
        print(f"\033[91m[FAIL] Could not find transaction {txn} in a block {block['number']} \033[0m")
        raise Exception
    
    print(f"\033[92m[OK]\033[0m Transation found in a block.")
    

    # Now check that block hash is correctly computed and that it contains all the transactions.
    # block hash is computed as a hash of block number, timestamp, previous block and rolling hash of all the included transactions.
    tx_rolling_hash = compute_transaction_rolling_hash(transactions_in_block)
    calculated_block_hash  = calculate_block_hash(tx['blockNumber'], block['timestamp'], block['parentHash'], tx_rolling_hash)
    if calculated_block_hash.hex() != block['hash'].hex():
        print(f"\033[91m[FAIL] Block hash doesn't match for {block['number']} \033[0m")
        raise 
    
    print(f"\033[92m[OK]\033[0m Block hash is valid")
    
    return tx['blockNumber'], block['hash'].hex(), int(block['l1BatchNumber'], 16)



def get_batch_root_hash(l1_batch):
    web3 = Web3(Web3.HTTPProvider(ZKSYNC_URL))
    # Check if connected successfully
    if not web3.is_connected():
        print("Failed to connect to zkSync node.")
        raise
    
    ethweb3 = Web3(Web3.HTTPProvider(ETH_URL))
    # Check if connected successfully
    if not ethweb3.is_connected():
        print("Failed to connect to zkSync node.")
        raise
    
    print(f"\033[92m[OK]\033[0m Connected to {ZKSYNC_URL} and {ETH_URL}")
    
    l1_address = get_l1_address()
    if l1_address not in WHITELISTED_ADDRESSES:
        print(f"\033[93m[WARNING] - Assuming L1 address of the contract is {l1_address} - please verify manually - https://etherscan.io/address/{l1_address} \033[0m")


    commitTx, proveTx, executeTx = get_commit_and_prove_and_verify(l1_batch)
    if commitTx is None:
        print(f"\033[91m[FAIL] Batch {l1_batch} is not commited yet - please try later. \033[0m")
        raise 
    
    
    # check that commitTx is of the right type.
        # Fetch the transaction
    try:
        tx = ethweb3.eth.get_transaction(commitTx)
    except Exception as e:
        print(f"An error occurred: {e}")
        raise
    
    try:
        receipt = ethweb3.eth.get_transaction_receipt(commitTx)
    except Exception as e:
        print(f"An error occurred: {e}")
        raise
    
    if receipt.status != 1:
        print(f"\033[91m[FAIL] L1 commit tx {commitTx} is not successful. \033[0m")
        raise Exception
    
    if receipt.to != l1_address:
        # It should be a 'fail' - but currently we are sending the transactions to validator lock and NOT to the proxy.
        if receipt.to not in WHITELISTED_ADDRESSES:
            print(f"\033[93m[WARNING] - L1 commit tx {commitTx} is being sent to a different address: - please verify manually - https://etherscan.io/address/{receipt.to} \033[0m")

    (new_state_root, _) = parse_commitcall_calldata(tx['input'], l1_batch)

    if proveTx is None:
        print(f"\033[95m[WARN] Batch {l1_batch} is not proven yet. Make sure to re-run the tool later. \033[0m")
        is_proven = False
    else:

        try:
            prove_tx = ethweb3.eth.get_transaction(proveTx)
        except Exception as e:
            print(f"An error occurred: {e}")
            raise
    
        try:
            prove_receipt = ethweb3.eth.get_transaction_receipt(proveTx)
        except Exception as e:
            print(f"An error occurred: {e}")
            raise
        
        if prove_receipt.to != receipt.to:
            print(f"\033[91m[FAIL] L1 commit tx was sent to different address than prove ts {receipt.to} vs {prove_receipt.to}. \033[0m")
            raise Exception    
        
        if prove_receipt.status != 1:
            print(f"\033[91m[FAIL] L1 prove tx {proveTx} is not successful. \033[0m")
            raise Exception
        
        
    
    
        batch_hash = parse_provecall_calldata(prove_tx['input'], l1_batch)
        if batch_hash != new_state_root:
            print(f"\033[91m[FAIL] Prove hash {batch_hash} doesn't match commit hash {new_state_root}. \033[0m")
            raise Exception
        
        is_proven = True


    
    return is_proven, new_state_root

    
    

def get_l1_address():
    headers = {"Content-Type": "application/json"}
    data = {"jsonrpc": "2.0", "id": 1, "method": "zks_getMainContract", "params": []}
    response = requests.post(ZKSYNC_URL, headers=headers, data=json.dumps(data))
    return response.json()["result"]

def get_commit_and_prove_and_verify(l1_batch):
    headers = {"Content-Type": "application/json"}
    data = {"jsonrpc": "2.0", "id": 1, "method": "zks_getL1BatchDetails", "params": [l1_batch]}
    response = requests.post(ZKSYNC_URL, headers=headers, data=json.dumps(data))
    response_json = response.json()["result"]
    return response_json["commitTxHash"], response_json["proveTxHash"], response_json["executeTxHash"]



def compute_transaction_rolling_hash(transaction_hashes):
    prev = "0x" + "00" * 32

    for transaction in transaction_hashes:
        prev = Web3.solidity_keccak(['bytes32', 'bytes32'], [prev, transaction])
    return prev


def calculate_block_hash(block_number, block_timestamp, prev_block_hash, transaction_rolling_hash):
    return Web3.solidity_keccak(['uint256', 'uint256', 'bytes32', 'bytes32'], [block_number, block_timestamp, prev_block_hash, transaction_rolling_hash])







COMMIT_BATCHES_SELECTOR = "0x701f58c5"
#    function commitBatches(
#        StoredBatchInfo calldata _lastCommittedBatchData,
#        CommitBatchInfo[] calldata _newBatchesData
#    )

def parse_commitcall_calldata(calldata, batch_to_find):
    selector = calldata[0:4]

    if selector.hex() != COMMIT_BATCHES_SELECTOR:
        print(f"\033[91m[FAIL] Invalid selector {selector.hex()} - expected {COMMIT_BATCHES_SELECTOR}. \033[0m")
        raise Exception
    
    (last_commited_batch_data_, new_batches_data) = decode(["(uint64,bytes32,uint64,uint256,bytes32,bytes32,uint256,bytes32)", "(uint64,uint64,uint64,bytes32,uint256,bytes32,bytes32,bytes32,bytes,bytes)[]"], calldata[4:])

    # We might be commiting multiple batches in one call - find the one that we're looking for
    selected_batch = None
    for batch in new_batches_data:
        if batch[0] == batch_to_find:
            selected_batch = batch
    
    if not selected_batch:
        print(f"\033[91m[FAIL] Could not find batch {batch_to_find} in calldata.. \033[0m")
        raise Exception
    
    (batch_number_, timestamp_, index_repeated_storage_changes_, new_state_root_, num_l1_tx_, priority_op_hash_, bootloader_initial_heap_, events_queue_state_, system_logs_, total_pubdata_) = selected_batch

    # Now we have to unpack the latest block hash.
    pubdata_info = parse_pubdata(total_pubdata_)
    return (new_state_root_, pubdata_info)



PROVE_BATCHES_SELECTOR = "0x7f61885c"

#    function proveBatches(
#        StoredBatchInfo calldata _prevBatch,
#        StoredBatchInfo[] calldata _committedBatches,
#        ProofInput calldata _proof
#    ) external;

def parse_provecall_calldata(calldata, batch_to_find):
    selector = calldata[0:4]

    if selector.hex() != PROVE_BATCHES_SELECTOR:
        print(f"\033[91m[FAIL] Invalid selector {selector.hex()} - expected {PROVE_BATCHES_SELECTOR}. \033[0m")
        raise Exception
    
    (prev_batch, commited_batches, proofs) = decode(["(uint64,bytes32,uint64,uint256,bytes32,bytes32,uint256,bytes32)", "(uint64,bytes32,uint64,uint256,bytes32,bytes32,uint256,bytes32)[]", "(uint256[],uint256[])"], calldata[4:])

    # We might be commiting multiple batches in one call - find the one that we're looking for
    selected_batch = None
    for batch in commited_batches:
        if batch[0] == batch_to_find:
            selected_batch = batch
    
    if not selected_batch:
        print(f"\033[91m[FAIL] Could not find batch {batch_to_find} in calldata.. \033[0m")
        raise Exception
    
    (batch_number_, batch_hash_, index_repeated_storage_changes_,  num_l1_tx_, priority_op_hash_, logs2_tree_root, timestamp_, commitment) = selected_batch

    return batch_hash_



def get_key_for_batch(batch_number):
    k = keccak.new(digest_bits=256)
    MAPPING_BATCH_POSITION_IN_SYSTEM_CONTEXT = 8
    key = format(batch_number, "064x") + format(MAPPING_BATCH_POSITION_IN_SYSTEM_CONTEXT, "064x")
    k.update(bytes.fromhex(key))
    return k.hexdigest()


def get_key_for_recent_block(block_number):
    MAPPING_RECENT_BLOCK_POSITION_IN_SYSTEM_CONTRACT = 11
    return format(block_number % 257 + MAPPING_RECENT_BLOCK_POSITION_IN_SYSTEM_CONTRACT, "064x")


SYSTEM_CONTEXT_ADDRESS = "0x000000000000000000000000000000000000800B"

def prove_tx_inclusion_in_chain(tx):
    (block_number, block_hash, batch) = verify_tx_inclusion_in_block(tx)
    # shortcut method - works only if there are less that 256 blocks in a batch.
    # in future - replace with something more stable, that looks at the chain of blocks.

    storage_proof = get_storage_proof(SYSTEM_CONTEXT_ADDRESS, get_key_for_recent_block(block_number), batch)
    # check that the values match.
    if storage_proof['value'] != block_hash:
        # this might happen if the batch has more than 256 blocks. (then we'll need to add more code)
        print(f"\033[91m[FAIL] Block hash doesn't match entry in storage (block hash: {block_hash}) storage {storage_proof['value']}  \033[0m")
        raise Exception
    
    is_proven, roothash = get_batch_root_hash(batch)
    
    print(f"\033[92m[OK]\033[0m Roothash is {roothash.hex()}. Is proven: {is_proven}")

    verify_storage_proof(SYSTEM_CONTEXT_ADDRESS, "0x" + get_key_for_recent_block(block_number), storage_proof['proof'], storage_proof['value'], storage_proof['index'],
                         "0x" + roothash.hex())
    
    if is_proven:
        print(f"\033[92m[OK]\033[0m Roothash is VALID and verified & proven on on L1.")
    else:
        print(f"\033[92m[OK]\033[0m Roothash is VALID and verified on L1. (but please wait for proof)")
        

#prove_tx_inclusion_in_chain('0x71dab3ace8c2f2f2810ec58d136e7efed145a87d6b3e6fbdc3db7222f2b50f54')
#prove_tx_inclusion_in_chain('0x23948b6dac5703849490ba5336e1ef682485a0c82614ee26aff449def6093717')

# prove_tx_inclusion_in_chain('0xb07cf51bb1fb788e9ab4961af203ce1057cf40f2781007ff06e7c66b6fc814be')


#results = verify_tx_inclusion_in_block(TRANSACTION_TO_PROVE)
#print(results)


#verify_block_inclusion_in_batch(23683393, '0xc3a179e5a230e7fe7f97491f8f3b6d22a196152afa2fd0aae542e2d733c003be')

#with open("tx_prover/testdata/prove_calldata.json") as f:
#    data = json.load(f)
#    calldata = bytes.fromhex(data["calldata"][2:])
#    parse_provecall_calldata(calldata, 392626)




#with open("tx_prover/testdata/calldata.json") as f:
#    data = json.load(f)
#    calldata = bytes.fromhex(data["calldata"][2:])

