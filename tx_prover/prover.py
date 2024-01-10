# tool to prove that given transaction was included in L2.

# step 1 - check the block hash
# step 2 - fetch the pubdata (to verify the block hashes)

from web3 import Web3


TRANSACTION_TO_PROVE = "0x849c9f33ecd4fddc6f11a270180e39d99386a6074c23fdfcb7cd6ad9034aa47e"
ZKSYNC_URL = 'https://mainnet.era.zksync.io'



# Returns batch number + batch position, block number + block position
def get_block_and_batch_number(txn):    
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
    
    return int(tx['l1BatchNumber'],16), tx['blockNumber'], 


def compare_block_hashes(block_number):
    web3 = Web3(Web3.HTTPProvider('https://mainnet.era.zksync.io'))

    # Check if connected successfully
    if not web3.is_connected():
        print("Failed to connect to zkSync node.")
        return
    try:
        block = web3.eth.get_block(block_number)
    except Exception as e:
        print(f"An error occurred: {e}")
        return
    
    print("\n\n\n")

    print(block)
    print(block['hash'])

    transactions_in_block = block['transactions']
    print("All transactions")
    print(transactions_in_block)

    tx_rolling_hash = compute_transaction_rolling_hash(transactions_in_block)
    print(f"Rolling hash: {tx_rolling_hash.hex()}")
    calculated_block_hash  = calculate_block_hash(block['number'], block['timestamp'], block['parentHash'], tx_rolling_hash)
    print(f"Calculated hash: {calculated_block_hash.hex()}")
    print(f"Block hash was: {block['hash'].hex()}")


def compute_transaction_rolling_hash(transaction_hashes):
    prev = "0x" + "00" * 32

    for transaction in transaction_hashes:
        prev = Web3.solidity_keccak(['bytes32', 'bytes32'], [prev, transaction])
    return prev


def calculate_block_hash(block_number, block_timestamp, prev_block_hash, transaction_rolling_hash):
    return Web3.solidity_keccak(['uint256', 'uint256', 'bytes32', 'bytes32'], [block_number, block_timestamp, prev_block_hash, transaction_rolling_hash])




def get_transaction_hashes(web3_url, block_number):
    # Connect to the Ethereum node
    web3 = Web3(Web3.HTTPProvider(web3_url))

    # Check if connected successfully
    if not web3.is_connected():
        print("Failed to connect to Ethereum node.")
        return

    # Fetch the block
    try:
        block = web3.eth.get_block(block_number, full_transactions=True)
    except Exception as e:
        print(f"An error occurred: {e}")
        return

    # Extract transaction hashes
    transaction_hashes = [tx.hash.hex() for tx in block.transactions]
    return transaction_hashes

results = get_block_and_batch_number(TRANSACTION_TO_PROVE)
print(results)


#compare_block_hashes(23683391)

#compare_block_hashes(23683392)

#compare_block_hashes(23683393)


# Example usage
#web3_url = 'https://mainnet.era.zksync.io'  # Replace with your Ethereum node URL
#block_number = 1234567  # Replace with the block number you're interested in

#transaction_hashes = get_transaction_hashes(web3_url, block_number)
#if transaction_hashes is not None:
#    print("Transaction Hashes in Block:", transaction_hashes)
