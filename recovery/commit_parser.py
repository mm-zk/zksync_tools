import argparse
import sys
from dataclasses import dataclass, fields, field
from typing import List
from hexbytes import HexBytes
from eth_abi import decode
from enum import Enum

# ==========================================
# Data Structures
# ==========================================

L2_TO_L1_LOG_SERIALIZE_SIZE = 88

class SystemLogKey(Enum):
    L2_TO_L1_LOGS_TREE_ROOT_KEY = 0
    PACKED_BATCH_AND_L2_BLOCK_TIMESTAMP_KEY = 1
    CHAINED_PRIORITY_TXN_HASH_KEY = 2
    NUMBER_OF_LAYER_1_TXS_KEY = 3
    PREV_BATCH_HASH_KEY = 4
    L2_DA_VALIDATOR_OUTPUT_HASH_KEY = 5
    USED_L2_DA_VALIDATOR_ADDRESS_KEY = 6
    EXPECTED_SYSTEM_CONTRACT_UPGRADE_TX_HASH_KEY = 7
    UNKNOWN = 999

    @classmethod
    def from_int(cls, val):
        try:
            return cls(val)
        except ValueError:
            return cls.UNKNOWN

@dataclass
class ParsedSystemLog:
    index: int
    sender: str
    key_id: int
    key_name: str
    value: str

    def hex_encoded(self) -> str:
        return self.sender + self.key_id.to_bytes(4, byteorder='big').hex() + self.value

@dataclass
class BlobInfo:
    index: int
    opening_point: str
    claimed_value: str
    commitment: str
    proof: str
    is_prepublished: bool
    prepublished_hash: str

@dataclass
class ParsedDAInput:
    state_diff_hash: str
    full_pubdata_hash: str
    blobs_provided: int
    linear_hashes: List[str]    
    # Payload Fields
    da_mode: str  # "CALLDATA" or "BLOBS" or "UNKNOWN"
    # If Calldata
    pubdata: Optional[bytes] = None
    blob_tail_commitment: Optional[str] = None
    # If Blobs
    blobs: List[BlobInfo] = field(default_factory=list)

@dataclass
class StoredBatchInfo:
    batchNumber: int
    batchHash: str
    indexRepeatedStorageChanges: int
    numberOfLayer1Txs: int
    priorityOperationsHash: str
    l2LogsTreeRoot: str
    timestamp: int
    commitment: str

    @classmethod
    def from_tuple(cls, data):
        return cls(
            batchNumber=data[0],
            batchHash=cls._fmt_hex(data[1]),
            indexRepeatedStorageChanges=data[2],
            numberOfLayer1Txs=data[3],
            priorityOperationsHash=cls._fmt_hex(data[4]),
            l2LogsTreeRoot=cls._fmt_hex(data[5]),
            timestamp=data[6],
            commitment=cls._fmt_hex(data[7]),
        )
    
    @staticmethod
    def _fmt_hex(val: bytes) -> str:
        if isinstance(val, int): return hex(val)
        return "0x" + val.hex()

@dataclass
class CommitBatchInfo:
    batchNumber: int
    timestamp: int
    indexRepeatedStorageChanges: int
    newStateRoot: str
    numberOfLayer1Txs: int
    priorityOperationsHash: str
    bootloaderHeapInitialContentsHash: str
    eventsQueueStateHash: str
    systemLogs: str 
    operatorDAInput: str

    @classmethod
    def from_tuple(cls, data):
        obj = cls(
            batchNumber=data[0],
            timestamp=data[1],
            indexRepeatedStorageChanges=data[2],
            newStateRoot=cls._fmt_hex(data[3]),
            numberOfLayer1Txs=data[4],
            priorityOperationsHash=cls._fmt_hex(data[5]),
            bootloaderHeapInitialContentsHash=cls._fmt_hex(data[6]),
            eventsQueueStateHash=cls._fmt_hex(data[7]),
            systemLogs=cls._fmt_hex(data[8]),
            operatorDAInput=cls._fmt_hex(data[9]),
        )
        return obj

    def parsed_system_log(self, key: SystemLogKey) -> ParsedSystemLog:
        logs = self.parsed_system_logs()
        for log in logs:
            if log.key_name == key.name:
                return log
        return None
    
    def parsed_system_logs(self) -> List[ParsedSystemLog]:
        # 1. Parse System Logs
        log_bytes = HexBytes(self.systemLogs)
        parsed_logs = []
        # Each log is 88 bytes
        LOG_SIZE = 88
        count = len(log_bytes) // LOG_SIZE
        
        for i in range(count):
            start = i * LOG_SIZE
            # Structure: [4 bytes padding/flags] [20 bytes address] [32 bytes key] [32 bytes value]
            # Offset 4:24 = Address
            # Offset 24:56 = Key
            # Offset 56:88 = Value
            
            sender = "0x" + log_bytes[start+4 : start+24].hex()
            key_bytes = log_bytes[start+24 : start+56]
            val_bytes = log_bytes[start+56 : start+88]
            
            key_int = int.from_bytes(key_bytes, byteorder='big')
            key_enum = SystemLogKey.from_int(key_int)
            
            parsed_logs.append(ParsedSystemLog(
                index=i,
                sender=sender,
                key_id=key_int,
                key_name=key_enum.name,
                value="0x" + val_bytes.hex()
            ))
        return parsed_logs

    def parsed_da_input(self) -> ParsedDAInput:
        da_bytes = HexBytes(self.operatorDAInput)
        
        # Initialize result object with defaults
        result = ParsedDAInput(
            state_diff_hash="", full_pubdata_hash="", blobs_provided=0,
            linear_hashes=[], da_mode="UNKNOWN"
        )

        if len(da_bytes) < 65:
            print("operatorDAInput too short to contain header (min 65 bytes)")
            sys.exit(1)

        # --- 1. PARSE HEADER ---
        result.state_diff_hash = "0x" + da_bytes[0:32].hex()
        result.full_pubdata_hash = "0x" + da_bytes[32:64].hex()
        result.blobs_provided = da_bytes[64]

        # --- 2. PARSE LINEAR HASHES ---
        linear_hashes_start = 65
        linear_hashes_len = 32 * result.blobs_provided
        linear_hashes_end = linear_hashes_start + linear_hashes_len
        
        if len(da_bytes) < linear_hashes_end:
            print("operatorDAInput too short for linear hashes")
            sys.exit(1)

        linear_hashes_section = da_bytes[linear_hashes_start:linear_hashes_end]
        for i in range(result.blobs_provided):
            lh_start = i * 32
            lh = linear_hashes_section[lh_start : lh_start+32]
            result.linear_hashes.append("0x" + lh.hex())

        # --- 3. PARSE L1 DA INPUT ---
        l1_da_input = da_bytes[linear_hashes_end:]
        
        if len(l1_da_input) == 0:
             print("l1DaInput is empty")
             sys.exit(1)

        flag = l1_da_input[0]
        payload = l1_da_input[1:]

        if flag == 0:
            result.da_mode = "CALLDATA"
            if len(payload) >= 32:
                result.blob_tail_commitment = "0x" + payload[-32:].hex()
                result.pubdata = payload[:-32]
            else:
                print("Calldata payload too short (<32 bytes)")
                sys.exit(1)
        elif flag == 1:
            result.da_mode = "BLOBS"
            
            # 144 bytes (KZG) + 32 bytes (Pre-pub hash) = 176 bytes
            BLOB_DA_INPUT_SIZE = 176

            num_payload_blobs = len(payload) // BLOB_DA_INPUT_SIZE
            remainder = len(payload) % BLOB_DA_INPUT_SIZE
            
            if num_payload_blobs != result.blobs_provided:
                print(f"Header says {result.blobs_provided} blobs, payload has {num_payload_blobs}")
                sys.exit(1)
            
            if remainder != 0:
                print(f"Payload length {len(payload)} is not multiple of {BLOB_DA_INPUT_SIZE}")
                sys.exit(1)

            for b in range(num_payload_blobs):
                offset = b * BLOB_DA_INPUT_SIZE
                chunk = payload[offset : offset + BLOB_DA_INPUT_SIZE]
                
                # Parse Blob Chunk
                opening = chunk[:16]
                value = chunk[16:48]
                commit = chunk[48:96]
                proof = chunk[96:144]
                prepub = chunk[144:176]
                is_prepub = int.from_bytes(prepub, byteorder='big') != 0

                blob_info = BlobInfo(
                    index=b,
                    opening_point="0x" + opening.hex(),
                    claimed_value="0x" + value.hex(),
                    commitment="0x" + commit.hex(),
                    proof="0x" + proof.hex(),
                    is_prepublished=is_prepub,
                    prepublished_hash="0x" + prepub.hex()
                )
                result.blobs.append(blob_info)
        else:
            result.da_mode = f"UNKNOWN_FLAG_{flag}"
            print(f"Unknown DA Source flag: {flag}")
            sys.exit(1)
        return result
    
    @staticmethod
    def _fmt_hex(val: bytes) -> str:
        if isinstance(val, int): return hex(val)
        return "0x" + val.hex()

@dataclass
class CommitData:
    chain_id: int
    batch_from: int
    batch_to: int
    last_batch: StoredBatchInfo = None
    new_batches: List[CommitBatchInfo] = field(default_factory=list)

    def print_summary(self):
        print("\n" + "="*60)
        print(" COMMIT DATA")
        print("="*60)
        print(f"  chainId      : {self.chain_id}")
        print(f"  processFrom  : {self.batch_from}")
        print(f"  processTo    : {self.batch_to}")
        
        print("\n" + "="*60)
        print(" LAST COMMITTED BATCH (StoredBatchInfo)")
        print("="*60)
        for field in fields(self.last_batch):
            print(f"  {field.name:<35}: {getattr(self.last_batch, field.name)}")

        print("\n" + "="*60)
        print(f" NEW BATCHES (Count: {len(self.new_batches)})")
        print("="*60)
        for i, batch in enumerate(self.new_batches):
            print(f"\n--- Batch {batch.batchNumber} ---")
            for field in fields(batch):
                val = getattr(batch, field.name)
                print(f"  {field.name:<40}: {val}")
            print(f"  {'-'*20}")
            parsed_logs = batch.parsed_system_logs()
            print(f"  System Logs (parsed): {len(parsed_logs)} entries")
            for log in parsed_logs:
                print(f"    {log.index}: {log.sender} {log.key_name:<35} {log.value}")
            print(f"  {'-'*20}")
            da_input = batch.parsed_da_input()
            print(f"  Operator DA Input (parsed):")
            print(f"    State Diff Hash: {da_input.state_diff_hash}")
            print(f"    Full Pubdata Hash: {da_input.full_pubdata_hash}")
            print(f"    Blobs Provided: {da_input.blobs_provided}")
            print(f"    Linear Hashes: {da_input.linear_hashes}")
            print(f"    DA Mode: {da_input.da_mode}")
            if da_input.da_mode == "CALLDATA":
                print(f"    Pubdata: {da_input.pubdata.hex()}")
                print(f"    Blob Tail Commitment: {da_input.blob_tail_commitment}")
            elif da_input.da_mode == "BLOBS":
                print(f"    Blobs: {len(da_input.blobs)}")
                for blob in da_input.blobs:
                    print(f"    * Blob #{blob.index}:")
                    print(f"        Opening Point: {blob.opening_point}")
                    print(f"        Claimed Value: {blob.claimed_value}")
                    print(f"        Commitment: {blob.commitment}")
                    print(f"        Proof: {blob.proof}")
                    print(f"        Is Pre-published: {blob.is_prepublished}")
                    print(f"        Pre-published Hash: {blob.prepublished_hash}")
            print("-"*60)


# ==========================================
# Parsing Logic
# ==========================================

def parse_calldata(hex_data: str):
    if hex_data.startswith("0x"):
        hex_data = hex_data[2:]
    raw_bytes = HexBytes(hex_data)

    # commitBatchesSharedBridge(uint256 chainId, uint256 processFrom, uint256 processTo, bytes commitData)
    args_data = raw_bytes[4:]
    decoded_top = decode(['uint256', 'uint256', 'uint256', 'bytes'], args_data)
    chain_id = decoded_top[0]
    batch_from = decoded_top[1]
    batch_to = decoded_top[2]
    commit_blob = decoded_top[3]

    version = commit_blob[0]
    if version != 0:
        print(f"[!] Warning: Version byte is {version}, expected 0x00")    
    inner_payload = commit_blob[1:]

    # StoredBatchInfo
    stored_batch_info_abi = "(uint64,bytes32,uint64,uint256,bytes32,bytes32,uint256,bytes32)"
    # CommitBatchInfo
    commit_batch_info_abi = "(uint64,uint64,uint64,bytes32,uint256,bytes32,bytes32,bytes32,bytes,bytes)"
    types_list = [stored_batch_info_abi, f"{commit_batch_info_abi}[]"]

    try:
        decoded_inner = decode(types_list, inner_payload)
    except Exception as e:
        print(f"[!] Failed to decode commit data: {e}")
        sys.exit(1)

    last_batch = StoredBatchInfo.from_tuple(decoded_inner[0])
    new_batches = [CommitBatchInfo.from_tuple(x) for x in decoded_inner[1]]
    
    commit_data = CommitData(
        chain_id=chain_id,
        batch_from=batch_from,
        batch_to=batch_to,
        last_batch=last_batch,
        new_batches=new_batches
    )    
    return commit_data

# ==========================================
# Main
# ==========================================

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, help="File containing hex calldata")
    args = parser.parse_args()
    with open(args.input, 'r') as f:
        data = f.read().strip()
    commit_data = parse_calldata(data)
    commit_data.print_summary()
