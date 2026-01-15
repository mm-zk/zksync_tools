import hashlib
from web3 import Web3
from typing import List, Sequence
from pubdata_parser import L2ToL1Log

# Matches `MiniMerkleTree` default empty-leaf hash (keccak(zeros(88)))
EMPTY_TREE_LEAF_HASH = Web3.keccak(b"\x00" * 88)
BLOOM_BYTE_LEN = 256
TREE_SIZE = 16384  # For ProtocolVersion 28


def compute_local_root(l2_to_l1_logs: List[L2ToL1Log]) -> bytes:
    hashed_leaves = [Web3.keccak(_serialize_log(log)) for log in l2_to_l1_logs]
    if len(hashed_leaves) > TREE_SIZE:
        raise ValueError(f"{len(hashed_leaves)} logs do not fit into tree of size {TREE_SIZE}")
    padded = list(hashed_leaves)
    while len(padded) < TREE_SIZE:
        padded.append(EMPTY_TREE_LEAF_HASH)
    return _merkle_root_from_hashes(padded)


def update_rolling_hash(rolling_txs_hash, txs):
    for tx in txs:
        status = 0 if tx.is_error() else 1
        tx_status_commitment = Web3.keccak(bytes.fromhex(tx.hash()[2:]) + bytes([status])).hex()
        rolling_txs_hash = Web3.keccak(bytes.fromhex(rolling_txs_hash) + bytes.fromhex(tx_status_commitment)).hex()
    return rolling_txs_hash


def build_logs_bloom(logs):
    bloom = 0
    for log in logs:
        topics = log.topics() or []
        for topic in topics:
            if not topic:
                continue
            topic_bytes = _hex_to_bytes(topic, 32)
            bloom = _update_bloom(bloom, topic_bytes)
        address_bytes = _hex_to_bytes(log.address(), 20)
        bloom = _update_bloom(bloom, address_bytes)
    return bloom.to_bytes(BLOOM_BYTE_LEN, "big")


def hashed_key(address: bytes | str, key: bytes | str) -> bytes:
    if isinstance(address, str):
        address = bytes.fromhex(address.replace("0x", ""))
    if isinstance(key, str):
        key = bytes.fromhex(key.replace("0x", ""))
    if len(address) != 20:
        raise ValueError("address must be 20 bytes (H160)")
    if len(key) != 32:
        raise ValueError("key must be 32 bytes (H256)")
        
    # Blake2s-256 hash
    buf = bytearray(64)
    buf[12:32] = address
    buf[32:64] = key
    return hashlib.blake2s(buf, digest_size=32).digest()


def _hex_to_bytes(val, expected_len=None):
    if val is None:
        return None
    if isinstance(val, bytes):
        data = val
    else:
        if val.startswith("0x"):
            val = val[2:]
        if len(val) % 2 != 0:
            val = "0" + val
        data = bytes.fromhex(val)
    if expected_len is not None:
        if len(data) > expected_len:
            raise ValueError(f"Value {val} exceeds expected length {expected_len}")
        if len(data) < expected_len:
            data = data.rjust(expected_len, b"\x00")
    return data


def _update_bloom(current_bloom: int, data: bytes) -> int:
    hashed = Web3.keccak(data)
    for offset in (0, 2, 4):
        bit = int.from_bytes(hashed[offset:offset + 2], "big") & 0x07FF
        current_bloom |= 1 << bit
    return current_bloom


def _serialize_log(log: L2ToL1Log) -> bytes:
    if not (0 <= log.shard_id < 256):
        raise ValueError(f"shard_id {shard_id} does not fit into a byte")
    is_service = b"\x01" if log.is_service else b"\x00"
    if not (0 <= log.tx_number_in_block < 2**16):
        raise ValueError(f"tx_index_in_l1_batch {log.tx_number_in_block} does not fit into 2 bytes")

    sender = _hex_to_bytes(log.sender, 20)
    key = _hex_to_bytes(log.key, 32)
    value = _hex_to_bytes(log.value, 32)

    if len(sender) != 20 or len(key) != 32 or len(value) != 32:
        raise ValueError("Unexpected log field sizes when serializing log leaf")

    return (
        log.shard_id.to_bytes(1, "big")
        + is_service
        + log.tx_number_in_block.to_bytes(2, "big")
        + sender
        + key
        + value
    )


def _merkle_root_from_hashes(hashes: Sequence[bytes]) -> bytes:
    if not hashes:
        return EMPTY_TREE_LEAF_HASH

    level = list(hashes)
    while len(level) > 1:
        next_level = []
        for i in range(0, len(level), 2):
            left = level[i]
            right = level[i + 1] if i + 1 < len(level) else EMPTY_TREE_LEAF_HASH
            next_level.append(Web3.keccak(left + right))
        level = next_level
    return level[0]
