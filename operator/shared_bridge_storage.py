import requests
import json
from web3 import Web3


def get_storage_at(zksync_url, account, key, block="latest"):
    headers = {"Content-Type": "application/json"}
    data = {"jsonrpc": "2.0", "id": 1, "method": "eth_getStorageAt", "params": [account, key, block if block == "latest" else hex(block)]}
    response = requests.post(zksync_url, headers=headers, data=json.dumps(data))
    return response.json()["result"]




# for l1 weth bridge
def get_chain_balance_info(l1_url, bridge_address, chain_id):
    # chain balance is on 0x11 mapping slot
    balance = get_storage_at(l1_url, bridge_address, Web3.solidity_keccak(['uint256', 'uint256'], [int(chain_id,16), 0x11]).hex())
    hyperbridge_enabled = get_storage_at(l1_url, bridge_address, Web3.solidity_keccak(['uint256', 'uint256'], [int(chain_id,16), 0x12]).hex())

    return {
        'balance': int(balance,16),
        'hyperbridge_enabled': int(hyperbridge_enabled,16)
    }