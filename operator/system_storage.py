import requests
import json


def get_storage_at(zksync_url, account, key, block):
    headers = {"Content-Type": "application/json"}
    data = {"jsonrpc": "2.0", "id": 1, "method": "eth_getStorageAt", "params": [account, key, hex(block)]}
    response = requests.post(zksync_url, headers=headers, data=json.dumps(data))
    print(response.json())
    return response.json()["result"]


def split_u128(data):
    print(type(data))
    hex_str = data.lstrip("0x").rjust(64, '0')
    return (parse_int(hex_str[:32]), parse_int(hex_str[32:]))

def parse_int(hex_bytes):
    return int(hex_bytes, 16)


def get_system_context_state(zksync_url, block):
    SYSTEM_CONTEXT_ADDRESS = "0x000000000000000000000000000000000000800b"
    
    chainId = get_storage_at(zksync_url, SYSTEM_CONTEXT_ADDRESS, key="0x00", block=block)    
    origin = get_storage_at(zksync_url, SYSTEM_CONTEXT_ADDRESS, key="0x01", block=block)
    gasPrice = parse_int(get_storage_at(zksync_url, SYSTEM_CONTEXT_ADDRESS, key="0x02", block=block))
    blockGasLimit = parse_int(get_storage_at(zksync_url, SYSTEM_CONTEXT_ADDRESS, key="0x03", block=block))
    coinbase = get_storage_at(zksync_url, SYSTEM_CONTEXT_ADDRESS, key="0x04", block=block)
    # difficulty - 05
    baseFee = get_storage_at(zksync_url, SYSTEM_CONTEXT_ADDRESS, key="0x06", block=block)
    (currentBatchInfoTimestamp, currentBatchInfoNumber)  = split_u128(get_storage_at(zksync_url, SYSTEM_CONTEXT_ADDRESS, key="0x07", block=block))
    # batch hashes mapping - 08
    (currentBlockInfoTimestamp, currentBlockInfoNumber)  = split_u128(get_storage_at(zksync_url, SYSTEM_CONTEXT_ADDRESS, key="0x09", block=block))
    # rolling hash - 0x0a
    # recent block hashes - 257 values
    (currentVirtualBlockInfoTimestamp, currentVirtualBlockInfoNumber) = split_u128(get_storage_at(zksync_url, SYSTEM_CONTEXT_ADDRESS, key="0x10c", block=block))
    # upgrade info - 0x10d







    return {
        'chainId': chainId,
        'origin': origin,
        'gasPrice': gasPrice,
        'blockGasLimit': blockGasLimit,
        'coinbase': coinbase,
        'baseFee': baseFee,
        'currentBatchInfoTimestamp': currentBatchInfoTimestamp,
        'currentBatchInfoNumber': currentBatchInfoNumber,
        'currentBlockInfoTimestamp': currentBlockInfoTimestamp,
        'currentBlockInfoNumber': currentBlockInfoNumber,
        'currentVirtualBlockInfoTimestamp': currentVirtualBlockInfoTimestamp,
        'currentVirtualBlockInfoNumber': currentVirtualBlockInfoNumber,

    }


# TODO: add bytecode hashes of the system contracts