import requests
import json


def get_storage_at(zksync_url, account, key, block):
    headers = {"Content-Type": "application/json"}
    data = {"jsonrpc": "2.0", "id": 1, "method": "eth_getStorageAt", "params": [account, key, block if block == "latest" else hex(block)]}
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


def get_l1_state_storage(l1_url, l1_contract, block):
    governor = get_storage_at(l1_url, l1_contract, "0x7", block)
    # 0x8 - pending governor
    # 0x9 - mapping of validators
    verifier = get_storage_at(l1_url, l1_contract, "0xa", block)
    # 0xb - total batches exec
    # 0xc - total batches verified
    # 0xd - total batched commit
    # 0xe - stored batch hashes (mapping)
    # 0xf - l2Logs root mathes (mapping)
    # 0x10 - 12 - priority queue (mapping + start + end)
    # 0x13 - deprecated allow list
    # 0x14, 15, 16 - verifier params
    verifier_params = {
        'node': get_storage_at(l1_url, l1_contract, "0x14", block),
        'leaf': get_storage_at(l1_url, l1_contract, "0x15", block),
        'circuit': get_storage_at(l1_url, l1_contract, "0x16", block)
    }
    # 0x17 - bootloader hash
    # 0x18 - default account hash
    # 0x19 - zkporter
    # 0x1a - priorityTxmax limit
    # 0x1b - 0x1c (Upgrade storage - deprecated)
    # 0x1d - is withdraw finalized - mapping
    # 0x1e - deprecated - last limit
    # 0x1f - depreacated - withdrawn
    # 0x20 - deprecated total deposits (mapping)
    # 0x21 - protocol version
    # 0x22 - contracts upgrade tx hash
    # 0x23 - contracts upgrade batch number

    upgrade = {
        'tx_hash': get_storage_at(l1_url, l1_contract, "0x22", block),
        'batch': get_storage_at(l1_url, l1_contract, "0x23", block),
    }
    # 0x24 - admin
    # 0x25 - pending admin
    # 0x26 - pricing mode (new)


    return {
        'governor': governor,
        'verifier': verifier,
        'verifier_params': verifier_params,
        'upgrade': upgrade
    }