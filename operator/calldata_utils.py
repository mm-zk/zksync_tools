from eth_abi import decode
from pubdata import parse_pubdata

COMMIT_BATCHES_SELECTOR = "0x701f58c5"
#    function commitBatches(
#        StoredBatchInfo calldata _lastCommittedBatchData,
#        CommitBatchInfo[] calldata _newBatchesData
#    )


def parse_commitcall_calldata(calldata, batch_to_find):
    selector = calldata[0:4]

    if selector.hex() != COMMIT_BATCHES_SELECTOR:
        print(f"\033[91m[FAIL] Invalid selector {selector.hex()} - expected {COMMIT_BATCHES_SELECTOR}. \033[0m")
        raise Exception
    
    (last_commited_batch_data_, new_batches_data) = decode(["(uint64,bytes32,uint64,uint256,bytes32,bytes32,uint256,bytes32)", "(uint64,uint64,uint64,bytes32,uint256,bytes32,bytes32,bytes32,bytes,bytes)[]"], calldata[4:])

    # We might be commiting multiple batches in one call - find the one that we're looking for
    selected_batch = None
    for batch in new_batches_data:
        if batch[0] == batch_to_find:
            selected_batch = batch
    
    if not selected_batch:
        print(f"\033[91m[FAIL] Could not find batch {batch_to_find} in calldata.. \033[0m")
        raise Exception
    
    (batch_number_, timestamp_, index_repeated_storage_changes_, new_state_root_, num_l1_tx_, priority_op_hash_, bootloader_initial_heap_, events_queue_state_, system_logs_, total_pubdata_) = selected_batch

    # Now we have to unpack the latest block hash.
    pubdata_info = parse_pubdata(total_pubdata_)
    return (new_state_root_, pubdata_info)