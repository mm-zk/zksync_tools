import requests
import json
from web3 import Web3


def get_storage_at(zksync_url, account, key, block="latest"):
    headers = {"Content-Type": "application/json"}
    data = {"jsonrpc": "2.0", "id": 1, "method": "eth_getStorageAt", "params": [account, key, block if block == "latest" else hex(block)]}
    response = requests.post(zksync_url, headers=headers, data=json.dumps(data))
    return response.json()["result"]




# for l1 weth bridge
def get_chain_balance_info(l1_url, bridge_address, chain_id, base_token):
    ethweb3 = Web3(Web3.HTTPProvider(l1_url))
    # Check if connected successfully
    if not ethweb3.is_connected():
        print("Failed to connect to L1 node.")
        raise


    l1_bridge_abi = [
        {
            "name": "chainBalance",
            "inputs": [
                {
                    "type": "uint256",
                },
                {
                    "type": "address",
                }
            ],
            "outputs": [
                {
                    "type": "uint256"
                }
            ],
            "type": "function"
        },
    ]

    bridge_contract = ethweb3.eth.contract(bridge_address, abi=l1_bridge_abi)

    balance = bridge_contract.functions.chainBalance(int(chain_id, 16), base_token).call()

    # chain balance is on 0x11 mapping slot
    #balance = get_storage_at(l1_url, bridge_address, Web3.solidity_keccak(['uint256', 'uint256'], [int(chain_id,16), 0x11]).hex())
    #hyperbridge_enabled = get_storage_at(l1_url, bridge_address, Web3.solidity_keccak(['uint256', 'uint256'], [int(chain_id,16), 0x12]).hex())



    return {
        'balance': balance, #int(balance,16),
        # doesn't work for now.
        'hyperbridge_enabled': 0 #int(hyperbridge_enabled,16)
    }