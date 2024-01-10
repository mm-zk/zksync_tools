# tool to prove that given transaction was included in L2.

# step 1 - check the block hash
# step 2 - fetch the pubdata (to verify the block hashes)

from web3 import Web3


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

# Example usage
web3_url = 'https://mainnet.era.zksync.io'  # Replace with your Ethereum node URL
block_number = 1234567  # Replace with the block number you're interested in

transaction_hashes = get_transaction_hashes(web3_url, block_number)
if transaction_hashes is not None:
    print("Transaction Hashes in Block:", transaction_hashes)
