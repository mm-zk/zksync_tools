from enum import Enum


PackingType = Enum('PackingType', ['Add', 'Subtract', 'Replace'])

def unpack_value(data, index):
    packing_type = int.from_bytes(data[index: index+1], 'big')
    index += 1
    packing_length = packing_type >> 3
    # last 3 bits
    packing_type  = packing_type & 0x7
    result_type = PackingType.Replace
    if packing_type == 0:
        result_type = PackingType.Replace
        # In this case, the key is the full length
        packing_length = 32
    if packing_type == 1:
        result_type = PackingType.Add
    if packing_type == 2:
        result_type = PackingType.Subtract
    if packing_type == 3:
        result_type = PackingType.Replace


    val = data[index: index + packing_length]
    index += packing_length
    return index, result_type, val

    


# Returns a map with initial writes (key -> (type, value)) and repeated write (index -> (type, value)).
def parse_state_diff(state_diff, debug=False):
    index = 0

    version = int.from_bytes(state_diff[0:1], 'big')
    index += 1
    if debug:
        print(f"State diff version: {version}")
    total_logs_len = int.from_bytes(state_diff[index: index+3], 'big')
    index += 3
    if debug:
        print(f"State diff total logs len: {total_logs_len}")
    derived_key_size = int.from_bytes(state_diff[index: index+1], 'big')
    index +=1
    if debug:
        print(f"Derived key size: {derived_key_size}")

    ## initial writes
    initial_writes_count = int.from_bytes(state_diff[index: index+2], 'big')
    index +=2
    if debug:
        print(f"Initial writes count {initial_writes_count}")
    initial_writes = {}
    repeated_writes = {}
    for i in range(initial_writes_count):
        key = state_diff[index: index + 32]
        index += 32

        (index, result_type, value) =  unpack_value(state_diff, index)
        if debug:
            print(f"key : 0x..{key.hex()[-10:]} value: 0x..{value.hex()[-10:]}, type: {result_type}")
        initial_writes[key] = (result_type, value)

    if debug:
        print("Repeated writes")
    repeated_writes_count = 0

    while index < len(state_diff):
        key = state_diff[index: index + derived_key_size]
        index += derived_key_size
        (index, result_type, value) =  unpack_value(state_diff, index)
        if debug:
            print(f"key : 0x..{key.hex()[-10:]} value: 0x..{value.hex()[-10:]}, type: {result_type}")
        repeated_writes_count += 1
        repeated_writes[key] = (result_type, value)


    if debug:
        print(f"Repeated writes count: {repeated_writes_count}")
    return (initial_writes, repeated_writes)


    


def parse_pubdata(pubdata, debug=False):
    if debug:
        print(len(pubdata))
    # pubdata starts with number of l1 - l2 transactions.
    index = 0
    l1_l2_msg_counter = int.from_bytes(pubdata[0:4], 'big')
    index += 4
    if debug:
        print(l1_l2_msg_counter)
    SIZE_OF_L1_L2_MSG = 88
    index += l1_l2_msg_counter * SIZE_OF_L1_L2_MSG
    large_msg_counter = int.from_bytes(pubdata[index: index+4], 'big')
    index += 4 
    if debug:
        print(f"large msg: {large_msg_counter}")
    for _ in range(large_msg_counter):
        msg_size = int.from_bytes(pubdata[index: index+4], 'big')
        if debug:
            print(f"msg size: {msg_size}")
        index += 4 + msg_size

    length_of_messages = index 
    bytecodes_size = int.from_bytes(pubdata[index: index+4], 'big')
    index += 4 
    if debug:
        print(f"bytecodes: {bytecodes_size}")
    for _ in range(bytecodes_size):
        msg_size = int.from_bytes(pubdata[index: index+4], 'big')
        print(f"bytecode size: {msg_size}")
        index += 4 + msg_size 

    length_of_bytecodes = index - length_of_messages

    state_diff = pubdata[index:]
    if debug:
        print(f"State diff size: {len(state_diff)}")
    (initial_writes, repeated_writes) = parse_state_diff(state_diff, debug)

    return (l1_l2_msg_counter, large_msg_counter, bytecodes_size, initial_writes, repeated_writes, 
            [length_of_messages, length_of_bytecodes, len(state_diff)])


