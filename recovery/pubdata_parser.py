import argparse
import dataclasses
import sys
import json
from dataclasses import asdict, dataclass, field
from typing import List

class EnhancedJSONEncoder(json.JSONEncoder):
    def default(self, o):
        if isinstance(o, bytes):
            return "0x" + o.hex()
        if dataclasses.is_dataclass(o):
            return asdict(o)
        return super().default(o)

# ==========================================
# Data Structures
# ==========================================

@dataclass
class L2ToL1Log:
    shard_id: int
    is_service: bool
    tx_number_in_block: int
    sender: str
    key: str
    value: str

@dataclass
class InitialWrite:
    derived_key: str
    operation: int
    value: int

@dataclass
class RepeatedWrite:
    index: int
    operation: int
    value: int

@dataclass
class StateDiff:
    version: int
    bytes_per_enumeration_index: int
    initial_writes: List[InitialWrite] = field(default_factory=list)
    repeated_writes: List[RepeatedWrite] = field(default_factory=list)

@dataclass
class Pubdata:
    l2_to_l1_logs: List[L2ToL1Log] = field(default_factory=list)
    messages: List[bytes] = field(default_factory=list)
    bytecodes: List[bytes] = field(default_factory=list)
    state_diff: StateDiff = None
    raw: dict = field(default_factory=lambda: {
        "l2_to_l1_logs": b'',
        "messages": b'',
        "bytecodes": b'',
        "state_diffs": b''
    })

    def get_raw(self) -> bytes:
        return self.raw["l2_to_l1_logs"] + self.raw["messages"] + self.raw["bytecodes"] + self.raw["state_diffs"]
    
    def get_raw_for_key(self, key: str) -> bytes:
        return self.raw[key]

    def print_summary(self):
        print("\n" + "="*60)
        print(" PUBDATA")
        print("="*60)

        # 1. L2 to L1 Logs
        print(f"\n[+] L2 to L1 logs: {len(self.l2_to_l1_logs)} entries")
        for i, log in enumerate(self.l2_to_l1_logs):
            print(f"    Log #{i}: Sender=0x{log.sender} Key=0x{log.key} Val=0x{log.value} Shard={log.shard_id} IsService={log.is_service} TxNum={log.tx_number_in_block}")

        # 2. Messages
        print(f"\n[+] Messages: {len(self.messages)} entries")
        for i, msg in enumerate(self.messages):
            print(f"    Msg #{i}: {msg.hex()}")

        # 3. Bytecodes
        print(f"\n[+] Bytecodes: {len(self.bytecodes)} entries")
        for i, bc in enumerate(self.bytecodes):
            print(f"    Bytecode #{i}: {len(bc)} bytes")

        # 4. State Diffs
        if self.state_diff:
            sd = self.state_diff
            print(f"\n[+] State Diffs")
            print(f"    Initial Writes: {len(sd.initial_writes)}")
            for i, iw in enumerate(sd.initial_writes):
                print(f"    * InitWrite #{i}: Key={iw.derived_key} Op={iw.operation} Val={iw.value}")
            
            print(f"    Repeated Writes: {len(sd.repeated_writes)}")
            for i, rw in enumerate(sd.repeated_writes):
                print(f"    * RepWrite #{i}: Idx={rw.index} Op={rw.operation} Val={rw.value}")

# ==========================================
# Parsing Logic
# ==========================================

def read_compressed_value(data, offset):
    """
    Decodes the [Metadata Byte] + [Data] structure
    """
    metadata = data[offset]
    # Top 5 bits = Length
    length = metadata >> 3
    # Bottom 3 bits = Operation
    operation = metadata & 0x07
    if operation == 0:
        length = 32
    # Read the data
    val_bytes = data[offset+1 : offset+1+length]
    val_int = int.from_bytes(val_bytes, 'big')
    return operation, length, val_int


def parse_pubdata(data_hex: str) -> Pubdata:
    if data_hex.startswith("0x"):
        data_hex = data_hex[2:]
    
    data = bytearray.fromhex(data_hex)
    cursor = 0
    result = Pubdata()

    # --- 1. L2 to L1 logs ---
    start_of_l2_to_l1_logs = cursor
    num_l2_to_l1_logs = int.from_bytes(data[cursor:cursor+4], 'big')
    cursor += 4
    
    for i in range(num_l2_to_l1_logs):
        # 88 bytes: Shard(1)+IsService(1)+TxNum(2)+Sender(20)+Key(32)+Value(32)
        log_entry = data[cursor:cursor+88]
        cursor += 88
        
        shard_id = log_entry[0]
        is_service = (log_entry[1] > 0)
        tx_number_in_block = int.from_bytes(log_entry[2:4], 'big')
        sender = log_entry[4:24].hex()
        key = log_entry[24:56].hex()
        value = log_entry[56:88].hex()
        result.l2_to_l1_logs.append(L2ToL1Log(shard_id, is_service, tx_number_in_block, sender, key, value))
    
    result.raw["l2_to_l1_logs"] = bytes(data[start_of_l2_to_l1_logs:cursor])

    # --- 2. MESSAGES ---
    start_of_messages = cursor
    num_msgs = int.from_bytes(data[cursor:cursor+4], 'big')
    cursor += 4
    
    for i in range(num_msgs):
        msg_len = int.from_bytes(data[cursor:cursor+4], 'big')
        cursor += 4
        msg_body = data[cursor:cursor+msg_len]
        cursor += msg_len
        result.messages.append(bytes(msg_body))
    
    result.raw["messages"] = bytes(data[start_of_messages:cursor])

    # --- 3. BYTECODES ---
    start_of_bytecodes = cursor
    num_bytecodes = int.from_bytes(data[cursor:cursor+4], 'big')
    cursor += 4
    
    for i in range(num_bytecodes):
        bc_len = int.from_bytes(data[cursor:cursor+4], 'big')
        cursor += 4
        bc_body = data[cursor:cursor+bc_len]
        cursor += bc_len
        result.bytecodes.append(bytes(bc_body))
    
    result.raw["bytecodes"] = bytes(data[start_of_bytecodes:cursor])
    
    # --- 4. STATE DIFFS (Compressed) ---
    start_of_state_diffs = cursor
    # Parse header
    version = int.from_bytes(data[cursor:cursor+1], 'big')
    cursor += 1
    
    # Total length of state diff section
    state_diff_len = int.from_bytes(data[cursor:cursor+3], 'big')
    cursor += 3
    
    # Enumeration index size
    bytes_per_enum = int.from_bytes(data[cursor:cursor+1], 'big')
    cursor += 1
    
    end_of_state_diffs = cursor + state_diff_len
    
    result.state_diff = StateDiff(
        version=version, 
        bytes_per_enumeration_index=bytes_per_enum
    )

    # 4a. Initial Writes
    num_initial_writes = int.from_bytes(data[cursor:cursor+2], 'big')
    cursor += 2
    for i in range(num_initial_writes):
        # Structure: [DerivedKey (32b)] + [Compressed Value]
        derived_key = data[cursor:cursor+32].hex()
        cursor += 32        
        op_type, val_len, value_int = read_compressed_value(data, cursor)
        cursor += (1 + val_len)
        result.state_diff.initial_writes.append(
            InitialWrite(derived_key, op_type, value_int)
        )

    # 4b. Repeated Writes
    while cursor < end_of_state_diffs:
        # Structure: [Index (4b)] + [Compressed Value]
        if cursor + bytes_per_enum > end_of_state_diffs:
            print(f"    (End of data, found {end_of_state_diffs - cursor - bytes_per_enum} bytes of padding)")
            sys.exit(1)
        index = int.from_bytes(data[cursor:cursor+bytes_per_enum], 'big')
        cursor += bytes_per_enum
        op_type, val_len, value_int = read_compressed_value(data, cursor)
        cursor += (1 + val_len)
        result.state_diff.repeated_writes.append(
            RepeatedWrite(index, op_type, value_int)
        )
    result.raw["state_diffs"] = bytes(data[start_of_state_diffs:end_of_state_diffs])

    return result

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, help="File containing hex pubdata")
    parser.add_argument("--json", action="store_true", help="Output result as JSON")
    args = parser.parse_args()
    with open(args.input, 'r') as f:
        pubdata_input = f.read().strip()
    parsed = parse_pubdata(pubdata_input)
    if args.json:
        print(json.dumps(parsed, indent=2, cls=EnhancedJSONEncoder))
    else:
        parsed.print_summary()
