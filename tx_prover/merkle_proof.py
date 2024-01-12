from web3 import Web3
import requests
import json
from eth_abi import decode
from enum import Enum
import hashlib


TRANSACTION_TO_PROVE = "0x849c9f33ecd4fddc6f11a270180e39d99386a6074c23fdfcb7cd6ad9034aa47e"
ZKSYNC_URL = 'https://mainnet.era.zksync.io'
ETH_URL = 'https://rpc.ankr.com/eth'


# Fetches the storage proof for a given account, key, batch.
# In the response, you get the value + index (which is used for repeated writes), and proof (a list of siblings on merkle path).
def get_storage_proof(account, key, batch):
    headers = {"Content-Type": "application/json"}
    data = {"jsonrpc": "2.0", "id": 1, "method": "zks_getProof", "params": [account, [key], batch]}
    response = requests.post(ZKSYNC_URL, headers=headers, data=json.dumps(data))
    storage_proof = response.json()["result"]["storageProof"][0]
    return {'proof': storage_proof["proof"], 'value': storage_proof['value'], "index": storage_proof["index"]}


# Checks if given storage proof is valid (which means that it computes into the given roothash)
# Account, key, value, roothash - should be in hex format with 0x prefix.
# index should be an integer.
# proof should be a list of strings in hex format with 0x prefix.
 
def verify_storage_proof(account, key, proof, value, index, roothash, debug=False):
    if debug:
        print(f"Proof len: {len(proof)}")
    tree_key = bytes(12) + bytes.fromhex(account[2:]) + bytes.fromhex(key[2:])
    if len(tree_key) != 64:
        print(f"Wrong length {len(tree_key)} expected 64")
        raise Exception
    
    # this is the location in the merkle tree.
    tree_key_hash = hashlib.blake2s(tree_key).digest()
    

    empty_hash = hashlib.blake2s(bytes(40)).digest()
    
    encoded_value = index.to_bytes(8, byteorder='big') + bytes.fromhex(value[2:])
    if len(encoded_value) != 40:
        print(f"Wrong encoded value length: {len(encoded_value)} - expected 40.")
    value_hash = hashlib.blake2s(encoded_value).digest()
    

    # Now we go from the leaves all the way up to the root.
    depth = 255
    current_hash = value_hash
    for u64pos in range(0, len(tree_key_hash), 8):
        u64byte = int.from_bytes(tree_key_hash[u64pos: u64pos+8], 'little')
        # Bits are determining whether we are the left or right sibling.
        for i in range(64):
            bit = (u64byte>>(i))&1
            if len(proof) > depth:
                
                if len(proof[depth][2:]) != 64:
                    print(f"Wrong proof length {len(proof[depth][2:])} at {depth}")
                    raise Exception
                if debug:
                    print(f"Reading from depth {depth} bit is {bit}")
                other_hash = bytes.fromhex(proof[depth][2:])
            else:
                other_hash = empty_hash
            empty_hash = hashlib.blake2s(empty_hash + empty_hash).digest()
            if bit:
                if debug:
                    print(f"{depth} --> {other_hash.hex()[:6]} + {current_hash.hex()[:6]}")
                current_hash = hashlib.blake2s(other_hash + current_hash).digest()
            else:
                if debug:
                    print(f"{depth} --> {current_hash.hex()[:6]} + {other_hash.hex()[:6]}")
                current_hash = hashlib.blake2s(current_hash + other_hash).digest()
            depth -= 1

    
    if current_hash.hex() != roothash[2:]:
        print(f"Root hash doesn't match - proof is wrong - comparing {current_hash.hex()} with {roothash}")
        raise Exception
    if debug:
        print(f"Root hash is: {current_hash.hex()} - matching.")    
    

if True:
    #data = get_storage_proof("0x7F0d15C7FAae65896648C8273B6d7E43f58Fa842", "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b422", 354895)
    address = "0x000000000000000000000000000000000000800B"
    key = "0x0000000000000000000000000000000000000000000000000000000000000007"
    #key = "0x0000000000000000000000009fd660FDc82A13f2944A79d6A3F0218851c98De9"
    storage_proof = get_storage_proof(address, key, 354895)

    print(storage_proof)
    
    verify_storage_proof(address, key, storage_proof["proof"], storage_proof["value"], storage_proof["index"], "0xe5aaf538de0b0261e33190681aa8515fdec298bda95e6457fc30e6bf0460eb59")
