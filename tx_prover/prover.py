# tool to prove that given transaction was included in L2.

# step 1 - check the block hash
# step 2 - fetch the pubdata (to verify the block hashes)

from web3 import Web3
import requests
import json
from eth_abi import decode
from enum import Enum

TRANSACTION_TO_PROVE = "0x849c9f33ecd4fddc6f11a270180e39d99386a6074c23fdfcb7cd6ad9034aa47e"
ZKSYNC_URL = 'https://mainnet.era.zksync.io'
ETH_URL = 'https://rpc.ankr.com/eth'



# Checks that tx belongs to a block.
# Returns batch, block and block hash.
# does NOT 
def verify_tx_inclusion_in_block(txn):    
    web3 = Web3(Web3.HTTPProvider(ZKSYNC_URL))
    # Check if connected successfully
    if not web3.is_connected():
        print("Failed to connect to zkSync node.")
        return
    
    print(f"\033[92m[OK]\033[0m Connected to {ZKSYNC_URL}")

    # Fetch the transaction
    try:
        tx = web3.eth.get_transaction(txn)
    except Exception as e:
        print(f"An error occurred: {e}")
        return
    
    print(f"\033[92m[OK]\033[0m Transaction {txn} found. Checking block {tx['blockNumber']}")
    

    # now fetch the blockinfo
    try:
        block = web3.eth.get_block(tx['blockNumber'])
    except Exception as e:
        print(f"An error occurred: {e}")
        return
    
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
        raise Exception
    
    print(f"\033[92m[OK]\033[0m Block hash is valid")
    
    return tx['blockNumber'], block['hash'].hex()



def verify_block_inclusion_in_batch(block_number, block_hash):
    web3 = Web3(Web3.HTTPProvider(ZKSYNC_URL))
    # Check if connected successfully
    if not web3.is_connected():
        print("Failed to connect to zkSync node.")
        return
    
    ethweb3 = Web3(Web3.HTTPProvider(ETH_URL))
    # Check if connected successfully
    if not ethweb3.is_connected():
        print("Failed to connect to zkSync node.")
        return
    
    print(f"\033[92m[OK]\033[0m Connected to {ZKSYNC_URL} and {ETH_URL}")
    # now fetch the blockinfo
    try:
        block = web3.eth.get_block(block_number)
    except Exception as e:
        print(f"An error occurred: {e}")
        return
    
    if block['hash'].hex() != block_hash:
        print(f"\033[91m[FAIL] Block hash doesn't match \033[0m")
        raise Exception
    
    l1_batch = int(block['l1BatchNumber'],16)
    print(f"\033[92m[OK]\033[0m Checking if block {block_number} belongs to batch {l1_batch}")


    l1_address = get_l1_address()
    print(f"\033[93m[WARNING] - Assuming L1 address of the contract is {l1_address} - please verify manually - https://etherscan.io/address/{l1_address} \033[0m")


    commitTx, proveTx, executeTx = get_commit_and_prove_and_verify(l1_batch)
    if commitTx is None:
        print(f"\033[91m[FAIL] Batch {l1_batch} is not commited yet - please try later. \033[0m")
        raise Exception
    
    # check that commitTx is of the right type.
        # Fetch the transaction
    try:
        tx = ethweb3.eth.get_transaction(commitTx)
    except Exception as e:
        print(f"An error occurred: {e}")
        return
    
    #print(tx)
    # TODO: 
    # - check that it touched the right address
    # - check the ABI
    # - check that it was successful.

    calldata = tx['input']

    print(calldata)
    



    
    

    

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


PackingType = Enum('PackingType', ['Add', 'Subtract', 'Replace'])

def unpack_value(data, index):
    packing_type = int.from_bytes(data[index: index+1], 'big')
    index += 1
    packing_length = packing_type >> 3
    # last 3 bits
    packing_type  = packing_type & 0x7
    result_type = PackingType.Replace
    if packing_type == 0:
        result_type = PackingType.Replace
        # In this case, the key is the full length
        packing_length = 32
    if packing_type == 1:
        result_type = PackingType.Add
    if packing_type == 2:
        result_type = PackingType.Subtract
    if packing_type == 3:
        result_type = PackingType.Replace


    val = data[index: index + packing_length]
    return index, result_type, val

    


# Returns a map with initial writes (key -> (type, value)) and repeated write (index -> (type, value)).

def parse_state_diff(state_diff, debug=False):
    index = 0

    version = int.from_bytes(state_diff[0:1], 'big')
    index += 1
    if debug:
        print(f"State diff version: {version}")
    total_logs_len = int.from_bytes(state_diff[index: index+3], 'big')
    index += 3
    if debug:
        print(f"State diff total logs len: {total_logs_len}")
    derived_key_size = int.from_bytes(state_diff[index: index+1], 'big')
    index +=1
    if debug:
        print(f"Derived key size: {derived_key_size}")

    ## initial writes
    initial_writes_count = int.from_bytes(state_diff[index: index+2], 'big')
    index +=2
    if debug:
        print(f"Initial writes count {initial_writes_count}")
    initial_writes = {}
    repeated_writes = {}
    for i in range(initial_writes_count):
        key = state_diff[index: index + 32]
        index += 32

        (index, result_type, value) =  unpack_value(state_diff, index)
        if debug:
            print(f"key : 0x..{key.hex()[-10:]} value: 0x..{value.hex()[-10:]}, type: {result_type}")
        initial_writes[key] = (result_type, value)

    if debug:
        print("Repeated writes")
    repeated_writes_count = 0

    while index < len(state_diff):
        key = state_diff[index: index + derived_key_size]
        index += derived_key_size
        (index, result_type, value) =  unpack_value(state_diff, index)
        if debug:
            print(f"key : 0x..{key.hex()[-10:]} value: 0x..{value.hex()[-10:]}, type: {result_type}")
        repeated_writes_count += 1
        repeated_writes[key] = (result_type, value)


    if debug:
        print(f"Repeated writes count: {repeated_writes_count}")
    return (initial_writes, repeated_writes)


    








def parse_pubdata(pubdata):
    print(len(pubdata))
    # pubdata starts with number of l1 - l2 transactions.
    index = 0
    l1_l2_msg_counter = int.from_bytes(pubdata[0:4], 'big')
    index += 4
    print(l1_l2_msg_counter)
    SIZE_OF_L1_L2_MSG = 88
    index += l1_l2_msg_counter * SIZE_OF_L1_L2_MSG
    large_msg_counter = int.from_bytes(pubdata[index: index+4], 'big')
    index += 4 
    print(f"large msg: {large_msg_counter}")
    for _ in range(large_msg_counter):
        msg_size = int.from_bytes(pubdata[index: index+4], 'big')
        print(f"msg size: {msg_size}")
        index += 4 + msg_size 
    bytecodes_size = int.from_bytes(pubdata[index: index+4], 'big')
    index += 4 
    print(f"bytecodes: {bytecodes_size}")
    for _ in range(bytecodes_size):
        msg_size = int.from_bytes(pubdata[index: index+4], 'big')
        print(f"bytecode size: {msg_size}")
        index += 4 + msg_size 

    state_diff = pubdata[index:]
    print(f"State diff size: {len(state_diff)}")
    parse_state_diff(state_diff)










COMMIT_BATCHES_SELECTOR = "701f58c5"

#    function commitBatches(
#        StoredBatchInfo calldata _lastCommittedBatchData,
#        CommitBatchInfo[] calldata _newBatchesData
#    )

def parse_calldata(calldata):

    batch_to_find = 389674
    tx = "0x1234"


    selector = calldata[0:4]

    if selector.hex() != COMMIT_BATCHES_SELECTOR:
        print(f"\033[91m[FAIL] Invalid selector {selector.hex()} - expected {COMMIT_BATCHES_SELECTOR} in transation {tx}. \033[0m")
        raise Exception
    
    (last_commited_batch_data, new_batches_data) = decode(["(uint64,bytes32,uint64,uint256,bytes32,bytes32,uint256,bytes32)", "(uint64,uint64,uint64,bytes32,uint256,bytes32,bytes32,bytes32,bytes,bytes)[]"], calldata[4:])

    # We might be commiting multiple batches in one call - find the one that we're looking for
    selected_batch = None
    for batch in new_batches_data:
        if batch[0] == batch_to_find:
            selected_batch = batch
    
    if not selected_batch:
        print(f"\033[91m[FAIL] Could not find batch {batch_to_find} in calldata for transaction {tx}. \033[0m")
        raise Exception
    
    (batch_number_, timestamp_, index_repeated_storage_changes_, new_state_root_, num_l1_tx_, priority_op_hash_, bootloader_initial_heap_, events_queue_state_, system_logs_, total_pubdata_) = selected_batch

    # Now we have to unpack the latest block hash.
    
    parse_pubdata(total_pubdata_)


    #print(batch_number_)
    
    #struct CommitBatchInfo {
    #    uint64 batchNumber;
    #    uint64 timestamp;
    #    uint64 indexRepeatedStorageChanges;
    #    bytes32 newStateRoot;
    #    uint256 numberOfLayer1Txs;
    #    bytes32 priorityOperationsHash;
    #    bytes32 bootloaderHeapInitialContentsHash;
    #    bytes32 eventsQueueStateHash;
    #    bytes systemLogs;
    #    bytes totalL2ToL1Pubdata;
    #}


    


    #print(last_commited_batch_data)
    


        



#results = verify_tx_inclusion_in_block(TRANSACTION_TO_PROVE)
#print(results)


#verify_block_inclusion_in_batch(23683393, '0xc3a179e5a230e7fe7f97491f8f3b6d22a196152afa2fd0aae542e2d733c003be')


with open("tx_prover/testdata/calldata.json") as f:
    data = json.load(f)
    calldata = bytes.fromhex(data["calldata"][2:])

print(calldata[3])

print(calldata[0:4].hex())

parse_calldata(calldata)

#compare_block_hashes(23683391)

#compare_block_hashes(23683392)

#compare_block_hashes(23683393)


# Example usage
#web3_url = 'https://mainnet.era.zksync.io'  # Replace with your Ethereum node URL
#block_number = 1234567  # Replace with the block number you're interested in

#transaction_hashes = get_transaction_hashes(web3_url, block_number)
#if transaction_hashes is not None:
#    print("Transaction Hashes in Block:", transaction_hashes)
