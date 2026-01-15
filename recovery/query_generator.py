import argparse
import random
import datetime
import json
import binascii
import sys
from collections import defaultdict
from datetime import datetime, timezone
from enum import Enum
from web3 import Web3

from commit_parser import parse_calldata, CommitData, SystemLogKey
from stub_data import StubData
from pubdata_parser import parse_pubdata, Pubdata
from explorer_data import BatchDetails, ExplorerData
from tree import Tree
from utils import build_logs_bloom, compute_local_root, update_rolling_hash

# Assumptions:
# * No upgrade transactions
# * There is at least one batch in the database
# * All missing batches are committed, proven and executed on L1
# * Fee address didn't change 

PROTOCOL_VERSION = 28
FAIR_PUBDATA_PRICE_STUB = 1657824545
BLOCK_GAS_LIMIT = 1125899906842624
GAS_PER_PUBDATA_LIMIT = 50000
TOPIC_BLOCK_COMMIT = Web3.keccak(text="BlockCommit(uint256,bytes32,bytes32)") # 0x8f2916b2f2d78cc5890ead36c06c0f6d5d112c7e103589947e8e2f0d6eddb763

MASK_256 = 2**256
WITH_PROOFS = True

class EthTxType(Enum):
    COMMIT = "CommitBlocks"
    PROVE = "PublishProofBlocksOnchain"
    EXECUTE = "ExecuteBlocks"

class QueryGenerator:
    def _to_bytea(self, val):
        if val is None:
            return "NULL"
        if isinstance(val, bytes):
            val = val.hex()
        if val.startswith("0x"):
            val = val[2:]
        return f"'\\x{val}'"

    def _to_text(self, val):
        return f"'{val}'"

    def _to_ts(self, unix_ts):
        dt = datetime.fromtimestamp(unix_ts, timezone.utc)
        return "'" + dt.strftime('%Y-%m-%dT%H:%M:%S.%f') + "'"
    
    def _to_bytea_array(self, vals):
        return f"ARRAY[{','.join([self._to_bytea(val) for val in vals])}]::bytea[]"
    
    def _hex(self, length=20, prefix=True):
        """Generates a postgres bytea hex string"""
        val = binascii.b2a_hex(random.randbytes(length)).decode('utf-8')
        return f"'\\x{val}'" if prefix else f"\\x{val}"

    def _ts(self):
        """Generates a timestamp string"""
        return f"'{datetime.datetime.now().isoformat()}'"

    def _bool(self):
        return "TRUE" if random.choice([True, False]) else "FALSE"

    def _int(self, min_val=1, max_val=1000):
        return str(random.randint(min_val, max_val))
    
    def _bigint(self):
        return str(random.randint(1, 1000000))

    def _json(self):
        return f"'{json.dumps({'key': 'value'})}'"

    def _addr(self):
        return self._hex(20)

    def _hash(self):
        return self._hex(32)

    def _fmt(self, val, quote=False):
        if val is None: 
            return "NULL"
        if quote:
            return f"'{val}'"
        return str(val)

    # Table: eth_txs
    #
    #            Column                     |  Type   | Collation | Nullable |               Default               |
    #---------------------------------------+---------+-----------+----------+-------------------------------------+
    # id                                    | integer |           | not null | nextval('eth_txs_id_seq'::regclass) |
    # nonce                                 | bigint  |           | not null |                                     |
    # raw_tx                                | bytea   |           | not null |                                     |
    # contract_address                      | text    |           | not null |                                     |
    # tx_type                               | text    |           | not null |                                     |
    # gas_used                              | bigint  |           |          |                                     |
    # created_at                            | ts      |           | not null |                                     |
    # updated_at                            | ts      |           | not null |                                     |
    # has_failed                            | boolean |           | not null | false                               |
    # sent_at_block                         | integer |           |          |                                     |
    # confirmed_eth_tx_history_id           | integer |           |          |                                     |
    # predicted_gas_cost                    | bigint  |           |          | 0                                   |
    # from_addr                             | bytea   |           |          |                                     |
    # blob_sidecar                          | bytea   |           |          |                                     |
    # is_gateway                            | boolean |           | not null | false                               |
    # chain_id                              | bigint  |           |          |                                     |
    # status                                | text    |           |          |                                     |
    #---------------------------------------+---------+-----------+----------+-------------------------------------+
    def gen_eth_txs(self, tx_data, tx_type: EthTxType):
        tx_receipt = w3.eth.get_transaction_receipt(tx_data.hash)
        block = w3.eth.get_block(tx_data.blockNumber)
        
        nonce = tx_data.nonce
        raw_tx = self._to_bytea(tx_data.input.hex())
        contract_address = self._to_text(tx_data.to.lower())
        tx_type = self._to_text(tx_type.value)
        gas_used = tx_receipt.gasUsed
        created_at = self._to_ts(block.timestamp)
        updated_at = self._to_ts(block.timestamp)
        has_failed = "FALSE"
        sent_at_block = tx_data.blockNumber
        predicted_gas_cost = tx_data.gas
        from_addr = self._to_bytea(tx_data.get('from'))
        blob_sidecar = self._to_bytea('0x') if tx_type == EthTxType.COMMIT else "NULL" # OMITTED: not required, if sent already
        is_gateway = "FALSE"
        chain_id = tx_data.chainId

        cols = """(
            nonce, raw_tx, contract_address, tx_type, gas_used, created_at, updated_at,
            has_failed, sent_at_block, predicted_gas_cost, from_addr, blob_sidecar,
            is_gateway, chain_id
        )"""
        vals = f"""(
            {nonce}, {raw_tx}, {contract_address}, {tx_type}, {gas_used}, {created_at}, {updated_at}, 
            {has_failed}, {sent_at_block}, {predicted_gas_cost}, {from_addr}, {blob_sidecar}, 
            {is_gateway}, {chain_id}
        )"""
        return f"""
          INSERT INTO eth_txs {cols} VALUES {vals};
        """

    # Table: eth_txs_history
    #
    #            Column                     |  Type   | Collation | Nullable |               Default                       |
    #---------------------------------------+---------+-----------+----------+---------------------------------------------+
    # id                                    | integer |           | not null | nextval('eth_txs_history_id_seq'::regclass) |
    # eth_tx_id                             | integer |           | not null | nextval('eth_txs_history_eth_tx_id_seq')    |
    # tx_hash                               | text    |           | not null |                                             |
    # created_at                            | ts      |           | not null |                                             |
    # updated_at                            | ts      |           | not null |                                             |
    # base_fee_per_gas                      | bigint  |           | not null |                                             |
    # priority_fee_per_gas                  | bigint  |           | not null |                                             |
    # confirmed_at                          | ts      |           |          |                                             |
    # signed_raw_tx                         | bytea   |           |          |                                             |
    # sent_at_block                         | integer |           |          |                                             |
    # sent_at                               | ts      |           |          |                                             |
    # blob_base_fee_per_gas                 | bigint  |           |          |                                             |
    # max_gas_per_pubdata                   | bigint  |           |          |                                             |
    # predicted_gas_limit                   | bigint  |           |          |                                             |
    # sent_successfully                     | boolean |           | not null | true                                        |
    # finality_status                       | text    |           | not null | 'pending'::text                             |
    #---------------------------------------+---------+-----------+----------+---------------------------------------------+
    def gen_eth_txs_history(self, tx_data):
        block = w3.eth.get_block(tx_data.blockNumber)

        eth_tx_id = "(SELECT MAX(id) FROM eth_txs)"
        tx_hash = self._to_text("0x" + tx_data.hash.hex())
        created_at = self._to_ts(block.timestamp)
        updated_at = self._to_ts(block.timestamp)
        base_fee_per_gas = tx_data.maxFeePerGas
        priority_fee_per_gas = tx_data.maxPriorityFeePerGas
        confirmed_at = self._to_ts(block.timestamp)
        signed_raw_tx = self._to_bytea('0x')  # OMITTED: not required, if sent already
        sent_at_block = tx_data.blockNumber
        sent_at = self._to_ts(block.timestamp)
        blob_base_fee_per_gas = 1  # STUB: not required, can be fixed later
        predicted_gas_limit = tx_data.gas
        sent_successfully = "TRUE"
        finality_status = self._to_text("finalized")

        cols = """(
            eth_tx_id, tx_hash, created_at, updated_at, base_fee_per_gas, priority_fee_per_gas,
            confirmed_at, sent_at_block, sent_at, blob_base_fee_per_gas, predicted_gas_limit,
            sent_successfully, finality_status
        )"""
        vals = f"""(
            {eth_tx_id}, {tx_hash}, {created_at}, {updated_at}, {base_fee_per_gas}, {priority_fee_per_gas},
            {confirmed_at}, {sent_at_block}, {sent_at}, {blob_base_fee_per_gas}, {predicted_gas_limit},
            {sent_successfully}, {finality_status}
        )"""
        return f"""
          INSERT INTO eth_txs_history {cols} VALUES {vals};
          UPDATE eth_txs SET confirmed_eth_tx_history_id = (SELECT MAX(id) FROM eth_txs_history) WHERE id = {eth_tx_id};
        """

    # Table: l1_batches
    #
    #            Column                     |  Type     | Collation | Nullable |               Default                  |
    #---------------------------------------+-----------+-----------+----------+----------------------------------------+
    # number                                | bigint    |           | not null | nextval('blocks_number_seq'::regclass) |
    # timestamp                             | bigint    |           | not null |                                        |
    # is_sealed                             | boolean   |           | not null | true                                   |
    # l1_tx_count                           | integer   |           | not null |                                        |
    # l2_tx_count                           | integer   |           | not null |                                        |
    # bloom                                 | bytea     |           | not null |                                        |
    # priority_ops_onchain_data             | bytea[]   |           | not null |                                        |
    # hash                                  | bytea     |           |          |                                        |
    # commitment                            | bytea     |           |          |                                        |
    # eth_prove_tx_id                       | integer   |           |          |                                        |
    # eth_commit_tx_id                      | integer   |           |          |                                        |
    # eth_execute_tx_id                     | integer   |           |          |                                        |
    # created_at                            | ts        |           | not null |                                        |
    # updated_at                            | ts        |           | not null |                                        |
    # merkle_root_hash                      | bytea     |           |          |                                        |
    # l2_to_l1_logs                         | bytea[]   |           | not null | '{}'::bytea[]                          |
    # l2_to_l1_messages                     | bytea[]   |           | not null | '{}'::bytea[]                          |
    # predicted_commit_gas_cost             | bigint    |           | not null | 0                                      |
    # predicted_prove_gas_cost              | bigint    |           | not null | 0                                      |
    # predicted_execute_gas_cost            | bigint    |           | not null | 0                                      |
    # initial_bootloader_heap_content       | jsonb     |           | not null |                                        |
    # used_contract_hashes                  | jsonb     |           | not null |                                        |
    # compressed_initial_writes             | bytea     |           |          |                                        |
    # compressed_repeated_writes            | bytea     |           |          |                                        |
    # l2_l1_compressed_messages             | bytea     |           |          |                                        |
    # l2_l1_merkle_root                     | bytea     |           |          |                                        |
    # rollup_last_leaf_index                | bigint    |           |          |                                        |
    # zkporter_is_available                 | boolean   |           |          |                                        |
    # bootloader_code_hash                  | bytea     |           |          |                                        |
    # default_aa_code_hash                  | bytea     |           |          |                                        |
    # base_fee_per_gas                      | numeric   |           | not null | 1                                      |
    # aux_data_hash                         | bytea     |           |          |                                        |
    # pass_through_data_hash                | bytea     |           |          |                                        |
    # meta_parameters_hash                  | bytea     |           |          |                                        |
    # skip_proof                            | boolean   |           | not null | false                                  |
    # l1_gas_price                          | bigint    |           | not null | 0                                      |
    # l2_fair_gas_price                     | bigint    |           | not null | 0                                      |
    # protocol_version                      | integer   |           |          |                                        |
    # system_logs                           | bytea[]   |           | not null | '{}'::bytea[]                          |
    # compressed_state_diffs                | bytea     |           |          |                                        |
    # storage_refunds                       | bigint[]  |           |          |                                        |
    # pubdata_input                         | bytea     |           |          |                                        |
    # predicted_circuits                    | integer   |           |          |                                        |
    # predicted_circuits_by_type            | jsonb     |           |          |                                        |
    # pubdata_costs                         | bigint[]  |           |          |                                        |
    # tree_writes                           | bytea     |           |          |                                        |
    # fair_pubdata_price                    | bigint    |           | not null | 0                                      |
    # fee_address                           | bytea     |           | not null | '\x0000000000000000000000000000000000000000' |
    # evm_emulator_code_hash                | bytea     |           |          |                                        |
    # state_diff_hash                       | bytea     |           |          |                                        |
    # aggregation_root                      | bytea     |           |          |                                        |
    # local_root                            | bytea     |           |          |                                        |
    # batch_chain_merkle_path               | bytea     |           |          |                                        |
    # sealed_at                             | ts        |           |          |                                        |
    # final_precommit_eth_tx_id             | integer   |           |          |                                        |
    # pubdata_limit                         | bigint    |           |          |                                        |
    # blobs_amount                          | bigint    |           |          | 0                                      |
    # batch_chain_merkle_path_until_msg_root| bytea     |           |          |                                        |
    #---------------------------------------+-----------+-----------+----------+----------------------------------------+
    def gen_l1_batches(self, commit_data: CommitData, commit_tx, pubdata: Pubdata, batch_details: BatchDetails):
        tx_receipt = w3.eth.get_transaction_receipt(commit_tx.hash)
        tx_logs = {log.topics[0]: log for log in tx_receipt.logs}
        new_batch = commit_data.new_batches[0]
        encoded_system_logs = [new_batch.systemLogs[2:][i : i + 176] for i in range(0, len(new_batch.systemLogs[2:]), 176)]
        da_input = new_batch.parsed_da_input()

        number = new_batch.batchNumber
        timestamp = new_batch.timestamp
        is_sealed = "TRUE"
        l1_tx_count = new_batch.numberOfLayer1Txs
        l2_tx_count = batch_details.l2_tx_count()
        bloom = self._to_bytea("0"*512)
        priority_ops_onchain_data = StubData.get_l1_batches_priority_ops_onchain_data(number)
        hash = self._to_bytea(new_batch.newStateRoot)
        commitment = self._to_bytea(tx_logs[TOPIC_BLOCK_COMMIT].topics[3].hex())
        eth_commit_tx_id = f"(SELECT MAX(id) FROM eth_txs WHERE tx_type = '{EthTxType.COMMIT.value}')"
        eth_prove_tx_id = f"(SELECT MAX(id) FROM eth_txs WHERE tx_type = '{EthTxType.PROVE.value}')"
        eth_execute_tx_id = f"(SELECT MAX(id) FROM eth_txs WHERE tx_type = '{EthTxType.EXECUTE.value}')"
        created_at = self._to_ts(new_batch.timestamp)
        updated_at = self._to_ts(new_batch.timestamp)
        l2_to_l1_logs = "ARRAY[]::bytea[]" # SKIPPED: not sure why, but it's always empty in the database
        l2_to_l1_messages = self._to_bytea_array(pubdata.messages)
        initial_bootloader_heap_content = "'[]'" # SKIPPED: not required
        used_contract_hashes = "'[]'" # SKIPPED: not required
        l2_l1_merkle_root = self._to_bytea(new_batch.parsed_system_log(SystemLogKey.L2_TO_L1_LOGS_TREE_ROOT_KEY).value)
        rollup_last_leaf_index = new_batch.indexRepeatedStorageChanges
        zkporter_is_available = "FALSE"
        bootloader_code_hash = f"(SELECT bootloader_code_hash FROM l1_batches WHERE number = {number - 1})"
        default_aa_code_hash = f"(SELECT default_aa_code_hash FROM l1_batches WHERE number = {number - 1})"
        base_fee_per_gas = 1 # STUB: not required, can be fixed later
        aux_data_hash = self._to_bytea(f"0"*64) # SKIPPED: not required
        pass_through_data_hash = self._to_bytea(f"0"*64) # SKIPPED: not required
        meta_parameters_hash = f"(SELECT meta_parameters_hash FROM l1_batches WHERE number = {number - 1})"
        skip_proof = "FALSE"
        l1_gas_price = batch_details.l1_gas_price()
        l2_fair_gas_price = batch_details.l2_fair_gas_price()
        protocol_version = PROTOCOL_VERSION
        system_logs = self._to_bytea_array(encoded_system_logs)
        compressed_state_diffs = self._to_bytea(pubdata.get_raw_for_key("state_diffs"))
        storage_refunds = "ARRAY[]::bigint[]" # SKIPPED: not required
        pubdata_input = self._to_bytea(pubdata.get_raw())
        predicted_circuits_by_type = "'{}'::jsonb" # SKIPPED: not required
        pubdata_costs = "ARRAY[]::bigint[]" # SKIPPED: not required
        tree_writes = "NULL" # SKIPPED: can not be recovered without {address,key} for writes
        fair_pubdata_price = FAIR_PUBDATA_PRICE_STUB # STUB
        fee_address = f"(SELECT fee_address FROM l1_batches WHERE number = {number - 1})"
        evm_emulator_code_hash = f"(SELECT evm_emulator_code_hash FROM l1_batches WHERE number = {number - 1})"
        state_diff_hash = self._to_bytea(da_input.state_diff_hash)
        aggregation_root = f"(SELECT aggregation_root FROM l1_batches WHERE number = {number - 1})"
        local_root = self._to_bytea(compute_local_root(pubdata.l2_to_l1_logs).hex())
        sealed_at = self._to_ts(new_batch.timestamp)
        blobs_amount = da_input.blobs_provided

        cols = """(
            number, timestamp, is_sealed, l1_tx_count, l2_tx_count, bloom, 
            priority_ops_onchain_data, hash, commitment,
            eth_commit_tx_id, eth_prove_tx_id, eth_execute_tx_id,
            created_at, updated_at, 
            l2_to_l1_logs, l2_to_l1_messages, 
            initial_bootloader_heap_content, used_contract_hashes, l2_l1_merkle_root,
            rollup_last_leaf_index, zkporter_is_available, bootloader_code_hash, default_aa_code_hash, 
            base_fee_per_gas, aux_data_hash, pass_through_data_hash, meta_parameters_hash, skip_proof,
            l1_gas_price, l2_fair_gas_price, protocol_version,
            system_logs, compressed_state_diffs, storage_refunds, pubdata_input,
            predicted_circuits_by_type, pubdata_costs,
            tree_writes, fair_pubdata_price, fee_address, evm_emulator_code_hash,
            state_diff_hash, aggregation_root, local_root, sealed_at, blobs_amount
        )"""
        vals = f"""(
            {number}, {timestamp}, {is_sealed}, {l1_tx_count}, {l2_tx_count}, {bloom},
            {priority_ops_onchain_data}, {hash}, {commitment},
            {eth_commit_tx_id}, {eth_prove_tx_id}, {eth_execute_tx_id},
            {created_at}, {updated_at},
            {l2_to_l1_logs}, {l2_to_l1_messages},
            {initial_bootloader_heap_content}, {used_contract_hashes}, {l2_l1_merkle_root},
            {rollup_last_leaf_index}, {zkporter_is_available}, {bootloader_code_hash}, {default_aa_code_hash},
            {base_fee_per_gas}, {aux_data_hash}, {pass_through_data_hash}, {meta_parameters_hash}, {skip_proof},
            {l1_gas_price}, {l2_fair_gas_price}, {protocol_version},
            {system_logs}, {compressed_state_diffs}, {storage_refunds}, {pubdata_input},
            {predicted_circuits_by_type}, {pubdata_costs},
            {tree_writes}, {fair_pubdata_price}, {fee_address}, {evm_emulator_code_hash},
            {state_diff_hash}, {aggregation_root}, {local_root}, {sealed_at}, {blobs_amount}
        )"""
        return f"""
          INSERT INTO l1_batches {cols} VALUES {vals};
        """

    # Table: commitments
    #
    #            Column                     |  Type  | Collation | Nullable | Default |
    #---------------------------------------+--------+-----------+----------+---------+
    # l1_batch_number                       | bigint |           | not null |         |
    # events_queue_commitment               | bytea  |           |          |         |
    # bootloader_initial_content_commitment | bytea  |           |          |         |
    #---------------------------------------+--------+-----------+----------+---------+
    def gen_commitments(self, commit_data: CommitData):
        new_batch = commit_data.new_batches[0]
        l1_batch_number = new_batch.batchNumber
        events_queue_commitment = self._to_bytea(new_batch.eventsQueueStateHash)
        bootloader_initial_content_commitment = self._to_bytea(new_batch.bootloaderHeapInitialContentsHash)
        cols = "(l1_batch_number, events_queue_commitment, bootloader_initial_content_commitment)"
        vals = f"({l1_batch_number}, {events_queue_commitment}, {bootloader_initial_content_commitment})"
        return f"""
          INSERT INTO commitments {cols} VALUES {vals};
        """

    # Table: initial_writes
    #
    #            Column                     |  Type   | Collation | Nullable | Default |
    #---------------------------------------+---------+-----------+----------+---------+
    # hashed_key                            | bytea   |           | not null |         |
    # l1_batch_number                       | bigint  |           | not null |         |
    # created_at                            | ts      |           | not null |         |
    # updated_at                            | ts      |           | not null |         |
    # index                                 | bigint  |           | not null |         |
    #---------------------------------------+---------+-----------+----------+---------+
    def gen_initial_writes(self, commit_data: CommitData, pubdata: Pubdata):
        query = ""
        new_batch = commit_data.new_batches[0]
        for write in pubdata.state_diff.initial_writes:
            hashed_key = self._to_bytea(write.derived_key)
            l1_batch_number = new_batch.batchNumber
            created_at = self._to_ts(new_batch.timestamp)
            updated_at = self._to_ts(new_batch.timestamp)
            index = "((SELECT MAX(index) FROM initial_writes) + 1)"

            cols = "(hashed_key, l1_batch_number, created_at, updated_at, index)"
            vals = f"({hashed_key}, {l1_batch_number}, {created_at}, {updated_at}, {index})"
            query += f"INSERT INTO initial_writes {cols} VALUES {vals};\n"
        return query

    # Table: storage_logs
    #
    #            Column                     |  Type   | Collation | Nullable | Default |
    #---------------------------------------+---------+-----------+----------+---------+
    # hashed_key                            | bytea   |           | not null |         |
    # address                               | bytea   |           |          |         |
    # key                                   | bytea   |           |          |         |
    # value                                 | bytea   |           | not null |         |
    # operation_number                      | integer |           | not null |         |
    # tx_hash                               | bytea   |           |          |         |
    # miniblock_number                      | bigint  |           | not null |         |
    # created_at                            | ts      |           | not null |         |
    # updated_at                            | ts      |           | not null |         |
    #---------------------------------------+---------+-----------+----------+---------+
    def gen_storage_logs(self, commit_data: CommitData, pubdata: Pubdata, tree: Tree, batch_details: BatchDetails):
        # Assuming all the storage_logs belong to the same miniblock.
        # In theory, logs order is the only thing that matters.
        first_miniblock_number = batch_details.get_blocks()[0].block_number()
        new_batch = commit_data.new_batches[0]
        operation_number = 0
        query = ""
        
        # Initial writes
        for write in pubdata.state_diff.initial_writes:
            hashed_key = write.derived_key
            cur_value = 0
            if write.operation == 0 or write.operation == 3:
                new_value = write.value
            elif write.operation == 1:
                new_value = (cur_value + write.value) % MASK_256
            elif write.operation == 2:
                new_value = (cur_value - write.value) % MASK_256
            else:
                raise ValueError(f"Invalid operation: {write.operation}")
            new_value_hex = f"0x{new_value:064x}"

            # Update tree
            tree.set(hashed_key, new_value_hex)

            hashed_key_bytes = self._to_bytea(hashed_key)
            address = "NULL"
            key = "NULL"
            value = self._to_bytea(new_value_hex)
            tx_hash = "NULL"
            miniblock_number = first_miniblock_number
            created_at = self._to_ts(new_batch.timestamp)
            updated_at = self._to_ts(new_batch.timestamp)

            cols = f"""(
                hashed_key,
                address, key, value, operation_number,
                tx_hash, miniblock_number, created_at, updated_at
            )"""
            vals = f"""(
                {hashed_key_bytes},
                {address}, {key}, {value}, {operation_number},
                {tx_hash}, {miniblock_number}, {created_at}, {updated_at}
            )"""
            query += f"INSERT INTO storage_logs {cols} VALUES {vals};\n"
            operation_number += 1

        # Repeated writes
        for write in pubdata.state_diff.repeated_writes:
            hashed_key = tree.get_hashed_key(write.index)
            cur_value = int(tree.get_value(hashed_key), 16)
            if write.operation == 0 or write.operation == 3:
                new_value = write.value
            elif write.operation == 1:
                new_value = (cur_value + write.value) % MASK_256
            elif write.operation == 2:
                new_value = (cur_value - write.value) % MASK_256
            else:
                raise ValueError(f"Invalid operation: {write.operation}")
            new_value_hex = f"0x{new_value:064x}"
            
            # Update tree
            tree.set(hashed_key, new_value_hex)

            hashed_key_bytes = self._to_bytea(hashed_key)
            address = self._to_bytea(tree.get_address(hashed_key))
            key = self._to_bytea(tree.get_key(hashed_key))
            value = self._to_bytea(new_value_hex)
            tx_hash = "NULL"
            miniblock_number = first_miniblock_number
            created_at = self._to_ts(new_batch.timestamp)
            updated_at = self._to_ts(new_batch.timestamp)

            cols = f"""(
                hashed_key,
                address, key, value, operation_number,
                tx_hash, miniblock_number, created_at, updated_at
            )"""
            vals = f"""(
                {hashed_key_bytes},
                {address}, {key}, {value}, {operation_number},
                {tx_hash}, {miniblock_number}, {created_at}, {updated_at}
            )"""
            query += f"INSERT INTO storage_logs {cols} VALUES {vals};\n"
            operation_number += 1
        return query

    # Table: l2_to_l1_logs
    #
    #            Column                     |  Type   | Collation | Nullable | Default |
    #---------------------------------------+---------+-----------+----------+---------+
    # miniblock_number                      | bigint  |           | not null |         |
    # log_index_in_miniblock                | integer |           | not null |         |
    # log_index_in_tx                       | integer |           | not null |         |
    # tx_hash                               | bytea   |           | not null |         |
    # shard_id                              | integer |           | not null |         |
    # is_service                            | boolean |           | not null |         |
    # tx_index_in_miniblock                 | integer |           | not null |         |
    # tx_index_in_l1_batch                  | integer |           | not null |         |
    # sender                                | bytea   |           | not null |         |
    # key                                   | bytea   |           | not null |         |
    # value                                 | bytea   |           | not null |         |
    # created_at                            | ts      |           | not null |         |
    # updated_at                            | ts      |           | not null |         |
    #---------------------------------------+---------+-----------+----------+---------+
    def gen_l2_to_l1_logs(self, commit_data: CommitData, pubdata: Pubdata, batch_details: BatchDetails):
        query = ""
        new_batch = commit_data.new_batches[0]
        txs = batch_details.get_txs()
        cnt_logs_in_miniblock = defaultdict(int)
        cnt_logs_in_tx = defaultdict(int)

        for log in pubdata.l2_to_l1_logs:
            tx = txs[log.tx_number_in_block]
            tx_count_in_prev_miniblocks = next(i for i, t in enumerate(txs) if t.block_number() == tx.block_number())
            assert log.tx_number_in_block >= tx_count_in_prev_miniblocks
            
            miniblock_number = tx.block_number()
            log_index_in_miniblock = cnt_logs_in_miniblock[tx.block_number()]
            log_index_in_tx = cnt_logs_in_tx[tx.hash()]
            tx_hash = self._to_bytea(tx.hash())
            shard_id = int(log.shard_id)
            is_service = "TRUE" if log.is_service else "FALSE"
            tx_index_in_miniblock = log.tx_number_in_block - tx_count_in_prev_miniblocks
            tx_index_in_l1_batch = log.tx_number_in_block
            sender = self._to_bytea(log.sender)
            key = self._to_bytea(log.key)
            value = self._to_bytea(log.value)
            created_at = self._to_ts(new_batch.timestamp)
            updated_at = self._to_ts(new_batch.timestamp)
            cols = """(
                miniblock_number, log_index_in_miniblock, log_index_in_tx, tx_hash, 
                shard_id, is_service, tx_index_in_miniblock, tx_index_in_l1_batch, 
                sender, key, value, created_at, updated_at
            )"""
            vals = f"""(
                {miniblock_number}, {log_index_in_miniblock}, {log_index_in_tx}, {tx_hash},
                {shard_id}, {is_service}, {tx_index_in_miniblock}, {tx_index_in_l1_batch},
                {sender}, {key}, {value}, {created_at}, {updated_at}
            )"""
            query += f"""
                INSERT INTO l2_to_l1_logs {cols} VALUES {vals};\n
            """
            # Update counters
            cnt_logs_in_miniblock[tx.block_number()] += 1
            cnt_logs_in_tx[tx.hash()] += 1
        return query

    # Table: miniblocks
    #
    #            Column                     |  Type     | Collation | Nullable |               Default                      |
    #---------------------------------------+-----------+-----------+----------+--------------------------------------------+
    # number                                | bigint    |           | not null | nextval('miniblocks_number_seq'::regclass) |
    # l1_batch_number                       | bigint    |           |          |                                            |
    # timestamp                             | bigint    |           | not null |                                            |
    # hash                                  | bytea     |           | not null |                                            |
    # l1_tx_count                           | integer   |           | not null |                                            |
    # l2_tx_count                           | integer   |           | not null |                                            |
    # base_fee_per_gas                      | numeric   |           | not null |                                            |
    # gas_per_pubdata_limit                 | bigint    |           | not null |                                            |
    # created_at                            | ts        |           | not null |                                            |
    # updated_at                            | ts        |           | not null |                                            |
    # l1_gas_price                          | bigint    |           | not null | 0                                          |
    # l2_fair_gas_price                     | bigint    |           | not null | 0                                          |
    # bootloader_code_hash                  | bytea     |           |          |                                            |
    # default_aa_code_hash                  | bytea     |           |          |                                            |
    # protocol_version                      | integer   |           |          |                                            |
    # virtual_blocks                        | bigint    |           | not null | 0                                          |
    # fee_account_address                   | bytea     |           | not null | '\x0000000000000000000000000000000000000000' |
    # fair_pubdata_price                    | bigint    |           |          |                                            |
    # gas_limit                             | bigint    |           |          |                                            |
    # logs_bloom                            | bytea     |           |          |                                            |
    # evm_emulator_code_hash                | bytea     |           |          |                                            |
    # l2_da_validator_address               | bytea     |           | not null | '\x0000000000000000000000000000000000000000' |
    # pubdata_type                          | text      |           | not null | 'Rollup'::text                             |
    # rolling_txs_hash                      | bytea     |           |          |                                            |
    # eth_precommit_tx_id                   | integer   |           |          |                                            |
    #---------------------------------------+-----------+-----------+----------+--------------------------------------------+
    def gen_miniblocks(self, batch_details):
        query = ""
        blocks = batch_details.get_blocks()
        prev_miniblock_number = blocks[0].block_number() - 1
        rolling_txs_hash = "0"*64
        for block in blocks:
            # Build rolling_txs_hash and logs_bloom
            block_number = block.block_number()
            txs = batch_details.get_txs(block_number)
            rolling_txs_hash = update_rolling_hash(rolling_txs_hash, txs)
            logs = batch_details.get_logs(block_number)
            logs_bloom_bytes = build_logs_bloom(logs)            

            l1_batch_number = batch_details.batch_number()
            timestamp = block.timestamp()
            hash = self._to_bytea(block.hash())
            l1_tx_count = block.l1_tx_count()
            l2_tx_count = block.l2_tx_count()
            base_fee_per_gas = block.base_fee_per_gas()
            gas_per_pubdata_limit = GAS_PER_PUBDATA_LIMIT
            created_at = self._to_ts(block.timestamp())
            updated_at = self._to_ts(block.timestamp())
            l1_gas_price = batch_details.l1_gas_price()
            l2_fair_gas_price = batch_details.l2_fair_gas_price()
            bootloader_code_hash = f"(SELECT bootloader_code_hash FROM l1_batches WHERE number = {l1_batch_number})"
            default_aa_code_hash = f"(SELECT default_aa_code_hash FROM l1_batches WHERE number = {l1_batch_number})"
            protocol_version = PROTOCOL_VERSION
            virtual_blocks = 1
            fee_account_address = f"(SELECT fee_account_address FROM miniblocks WHERE number = {prev_miniblock_number})"
            fair_pubdata_price = FAIR_PUBDATA_PRICE_STUB
            gas_limit = BLOCK_GAS_LIMIT
            logs_bloom = self._to_bytea(logs_bloom_bytes)
            evm_emulator_code_hash = f"(SELECT evm_emulator_code_hash FROM l1_batches WHERE number = {l1_batch_number})"
            l2_da_validator_address = f"(SELECT l2_da_validator_address FROM miniblocks WHERE number = {prev_miniblock_number})"
            pubdata_type = self._fmt("Rollup", quote=True)
            eth_precommit_tx_id = "NULL"

            cols = """(
                number, l1_batch_number, timestamp, hash, l1_tx_count, l2_tx_count,
                base_fee_per_gas, gas_per_pubdata_limit, created_at, updated_at,
                l1_gas_price, l2_fair_gas_price, bootloader_code_hash, default_aa_code_hash,
                protocol_version, virtual_blocks, fee_account_address, fair_pubdata_price,
                gas_limit, logs_bloom, evm_emulator_code_hash, l2_da_validator_address,
                pubdata_type, rolling_txs_hash, eth_precommit_tx_id
            )"""
            vals = f"""(
                {block_number}, {l1_batch_number}, {timestamp}, {hash}, {l1_tx_count}, {l2_tx_count},
                {base_fee_per_gas}, {gas_per_pubdata_limit}, {created_at}, {updated_at},
                {l1_gas_price}, {l2_fair_gas_price}, {bootloader_code_hash}, {default_aa_code_hash},
                {protocol_version}, {virtual_blocks}, {fee_account_address}, {fair_pubdata_price},
                {gas_limit}, {logs_bloom}, {evm_emulator_code_hash}, {l2_da_validator_address},
                {pubdata_type}, {self._to_bytea(rolling_txs_hash)}, {eth_precommit_tx_id}
            )"""
            query += f"""
                INSERT INTO miniblocks {cols} VALUES {vals};\n
            """
        return query

    # Table: transactions
    #
    #            Column                     |  Type   | Collation | Nullable |               Default                |
    #---------------------------------------+---------+-----------+----------+--------------------------------------+
    # hash                                  | bytea   |           | not null |                                      |
    # is_priority                           | boolean |           | not null |                                      |
    # full_fee                              | numeric |           |          |                                      |
    # layer_2_tip_fee                       | numeric |           |          |                                      |
    # initiator_address                     | bytea   |           | not null |                                      |
    # nonce                                 | bigint  |           |          |                                      |
    # signature                             | bytea   |           |          |                                      |
    # input                                 | bytea   |           |          |                                      |
    # data                                  | jsonb   |           | not null |                                      |
    # received_at                           | ts      |           | not null |                                      |
    # priority_op_id                        | bigint  |           |          |                                      |
    # l1_batch_number                       | bigint  |           |          |                                      |
    # index_in_block                        | integer |           |          |                                      |
    # error                                 | varchar |           |          |                                      |
    # gas_limit                             | numeric |           |          |                                      |
    # gas_per_storage_limit                 | numeric |           |          |                                      |
    # gas_per_pubdata_limit                 | numeric |           |          |                                      |
    # tx_format                             | integer |           |          |                                      |
    # created_at                            | ts      |           | not null |                                      |
    # updated_at                            | ts      |           | not null |                                      |
    # execution_info                        | jsonb   |           | not null | '{}'::jsonb                          |
    # contract_address                      | bytea   |           |          |                                      |
    # in_mempool                            | boolean |           | not null | false                                |
    # l1_block_number                       | integer |           |          |                                      |
    # value                                 | numeric |           | not null | 0                                    |
    # paymaster                             | bytea   |           | not null |                                      |
    # paymaster_input                       | bytea   |           | not null |                                      |
    # max_fee_per_gas                       | numeric |           |          |                                      |
    # max_priority_fee_per_gas              | numeric |           |          |                                      |
    # effective_gas_price                   | numeric |           |          |                                      |
    # miniblock_number                      | bigint  |           |          |                                      |
    # l1_batch_tx_index                     | integer |           |          |                                      |
    # refunded_gas                          | bigint  |           | not null | 0                                    |
    # l1_tx_mint                            | numeric |           |          |                                      |
    # l1_tx_refund_recipient                | bytea   |           |          |                                      |
    # upgrade_id                            | integer |           |          |                                      |
    # timestamp_asserter_range_start        | ts      |           |          |                                      |
    # timestamp_asserter_range_end          | ts      |           |          |                                      |
    #---------------------------------------+---------+-----------+----------+--------------------------------------+
    def gen_transactions(self, batch_details: BatchDetails):
        query = ""
        txs = batch_details.get_txs()
        for i, tx in enumerate(txs):
            data_dict = {
                "value": hex(tx.value()),
                "calldata": tx.calldata(),
                "factoryDeps": tx.factory_deps(),
                "contractAddress": tx.to()
            }

            hash = self._to_bytea(tx.hash())
            is_priority = "TRUE" if tx.is_l1_tx() else "FALSE"
            full_fee = "0" if tx.is_l1_tx() else "NULL" 
            layer_2_tip_fee = "0" if tx.is_l1_tx() else "NULL"
            initiator_address = self._to_bytea(tx.initiator_address())
            nonce = "NULL" if tx.is_l1_tx() else tx.nonce()
            signature = self._to_bytea("0x") # Signatures are not stored
            input = self._to_bytea("0x")
            data = f"'{json.dumps(data_dict)}'::jsonb"
            received_at = self._to_ts(tx.timestamp())
            priority_op_id = self._fmt(StubData.priority_op_id(tx.hash()))
            l1_batch_number = batch_details.batch_number()
            index_in_block = tx.index_in_block()
            error = self._fmt(tx.error(), quote=True)
            gas_limit = tx.gas_limit()
            gas_per_storage_limit = "NULL"
            gas_per_pubdata_limit = tx.gas_per_pubdata_limit()
            tx_format = tx.tx_format()
            created_at = self._to_ts(tx.timestamp())
            updated_at = self._to_ts(tx.timestamp())
            execution_info = "'{}'::jsonb" # Not recoverable
            contract_address = self._to_bytea(tx.to())
            in_mempool = "FALSE"
            l1_block_number = self._fmt(StubData.l1_block_number(tx.hash()))
            value = tx.value()
            paymaster = self._to_bytea("0x0000000000000000000000000000000000000000") # Not recoverable
            paymaster_input = self._to_bytea("0x") # Not recoverable
            max_fee_per_gas = tx.max_fee_per_gas()
            max_priority_fee_per_gas = tx.max_priority_fee_per_gas()
            effective_gas_price = tx.effective_gas_price()
            miniblock_number = tx.block_number()
            l1_batch_tx_index = i
            refunded_gas = tx.gas_limit() - tx.gas_used()
            l1_tx_mint = self._fmt(StubData.l1_tx_mint(tx.hash()))
            l1_tx_refund_recipient = StubData.l1_tx_refund_recipient(tx.hash())
            upgrade_id = "NULL"
            timestamp_asserter_range_start = "NULL"
            timestamp_asserter_range_end = "NULL"

            cols = """(
                hash, is_priority, full_fee, layer_2_tip_fee, initiator_address, nonce, signature, input, data,
                received_at, priority_op_id, l1_batch_number, index_in_block, error, gas_limit, 
                gas_per_storage_limit, gas_per_pubdata_limit, tx_format, created_at, 
                updated_at, execution_info, contract_address, in_mempool, l1_block_number, value, paymaster, paymaster_input,
                max_fee_per_gas, max_priority_fee_per_gas, effective_gas_price, miniblock_number, l1_batch_tx_index, refunded_gas,
                l1_tx_mint, l1_tx_refund_recipient, upgrade_id, timestamp_asserter_range_start, timestamp_asserter_range_end
            )"""        
            vals = f"""(
                {hash}, {is_priority}, {full_fee}, {layer_2_tip_fee}, {initiator_address}, {nonce}, {signature}, {input}, {data},
                {received_at}, {priority_op_id}, {l1_batch_number}, {index_in_block}, {error}, {gas_limit},
                {gas_per_storage_limit}, {gas_per_pubdata_limit}, {tx_format}, {created_at}, {updated_at}, {execution_info},
                {contract_address}, {in_mempool}, {l1_block_number}, {value}, {paymaster}, {paymaster_input},
                {max_fee_per_gas}, {max_priority_fee_per_gas}, {effective_gas_price}, {miniblock_number}, {l1_batch_tx_index}, {refunded_gas},
                {l1_tx_mint}, {l1_tx_refund_recipient}, {upgrade_id}, {timestamp_asserter_range_start}, {timestamp_asserter_range_end}
            )"""
            query += f"""
                INSERT INTO transactions {cols} VALUES {vals};\n
            """
        return query

    # Table: events
    #
    #            Column                     |  Type   | Collation | Nullable |               Default                        |
    #---------------------------------------+---------+-----------+----------+----------------------------------------------+
    # miniblock_number                      | bigint  |           | not null |                                              |
    # tx_hash                               | bytea   |           | not null |                                              |
    # tx_index_in_block                     | integer |           | not null |                                              |
    # address                               | bytea   |           | not null |                                              |
    # event_index_in_block                  | integer |           | not null |                                              |
    # event_index_in_tx                     | integer |           | not null |                                              |
    # topic1                                | bytea   |           | not null |                                              |
    # topic2                                | bytea   |           | not null |                                              |
    # topic3                                | bytea   |           | not null |                                              |
    # topic4                                | bytea   |           | not null |                                              |
    # value                                 | bytea   |           | not null |                                              |
    # created_at                            | ts      |           | not null |                                              |
    # updated_at                            | ts      |           | not null |                                              |
    # tx_initiator_address                  | bytea   |           | not null | '\x0000000000000000000000000000000000000000' |
    #---------------------------------------+---------+-----------+----------+----------------------------------------------+
    def gen_events(self, batch_details: BatchDetails):
        logs = batch_details.get_logs()
        query = ""
        cnt_logs_in_block = defaultdict(int)
        for log in logs:
            tx = log.tx_details()
            topics = log.topics()

            miniblock_number = tx.block_number()
            tx_hash = self._to_bytea(tx.hash())
            tx_index_in_block = tx.index_in_block()
            address = self._to_bytea(log.address())
            event_index_in_block = cnt_logs_in_block[tx.block_number()]
            event_index_in_tx = log.log_index()
            topic1 = self._to_bytea(topics[0] or "0x" if len(topics) > 0 else "0x")
            topic2 = self._to_bytea(topics[1] or "0x" if len(topics) > 1 else "0x")
            topic3 = self._to_bytea(topics[2] or "0x" if len(topics) > 2 else "0x")
            topic4 = self._to_bytea(topics[3] or "0x" if len(topics) > 3 else "0x")
            value = self._to_bytea(log.data())
            created_at = self._to_ts(log.timestamp())
            updated_at = self._to_ts(log.timestamp())
            cols = """(
                miniblock_number, tx_hash, tx_index_in_block, address, event_index_in_block, 
                event_index_in_tx, topic1, topic2, topic3, topic4, value, created_at, updated_at
            )"""
            vals = f"""(
                {miniblock_number}, {tx_hash}, {tx_index_in_block}, {address}, {event_index_in_block},
                {event_index_in_tx}, {topic1}, {topic2}, {topic3}, {topic4}, {value}, {created_at}, {updated_at}
            )"""
            query += f"""
                INSERT INTO events {cols} VALUES {vals};\n
            """
            # Update counter
            cnt_logs_in_block[tx.block_number()] += 1
        return query

    # Table: factory_deps
    #
    #            Column                     |  Type   | Collation | Nullable | Default |
    #---------------------------------------+---------+-----------+----------+---------+
    # bytecode_hash                         | bytea   |           | not null |         |
    # bytecode                              | bytea   |           | not null |         |
    # miniblock_number                      | bigint  |           | not null |         |
    # created_at                            | ts      |           | not null |         |
    # updated_at                            | ts      |           | not null |         |
    #---------------------------------------+---------+-----------+----------+---------+
    def gen_factory_deps(self, commit_data: CommitData):
        # Generally, we should parse pubdata here to get factory deps.
        new_batch = commit_data.new_batches[0]
        factory_deps = StubData.get_factory_deps(new_batch.batchNumber)
        query = ""
        for factory_dep in factory_deps:
            bytecode_hash = self._to_bytea(factory_dep['bytecode_hash'])
            bytecode = self._to_bytea(factory_dep['bytecode'])
            miniblock_number = factory_dep['miniblock_number']
            created_at = self._fmt(factory_dep['created_at'], quote=True)
            updated_at = self._fmt(factory_dep['updated_at'], quote=True)
            
            cols = "(bytecode_hash, bytecode, miniblock_number, created_at, updated_at)"
            vals = f"({bytecode_hash}, {bytecode}, {miniblock_number}, {created_at}, {updated_at})"
            query += f"INSERT INTO factory_deps {cols} VALUES {vals};\n"
        return query

    # Table: processed_events
    #
    #            Column                     |  Type     | Collation | Nullable | Default |
    #---------------------------------------+-----------+-----------+----------+---------+
    # type                                  | event_type|           | not null |         |
    # chain_id                              | bigint    |           | not null |         |
    # next_block_to_process                 | bigint    |           | not null |         |
    #---------------------------------------+-----------+-----------+----------+---------+
    def gen_processed_events(self):
        return f"UPDATE processed_events SET next_block_to_process = {NEXT_BLOCK_TO_PROCESS};"

    # Table: proof_generation_details
    #
    #            Column                     |  Type   | Collation | Nullable |               Default                   |
    #---------------------------------------+---------+-----------+----------+-----------------------------------------+
    # l1_batch_number                       | bigint  |           | not null |                                         |
    # status                                | text    |           | not null |                                         |
    # proof_gen_data_blob_url               | text    |           |          |                                         |
    # proof_blob_url                        | text    |           |          |                                         |
    # created_at                            | ts      |           | not null |                                         |
    # updated_at                            | ts      |           | not null |                                         |
    # prover_taken_at                       | ts      |           |          |                                         |
    # vm_run_data_blob_url                  | text    |           |          |                                         |
    # proving_mode                          | enum    |           |          | 'proving_network'::proving_mode         |
    #---------------------------------------+---------+-----------+----------+-----------------------------------------+
    def gen_proof_generation_details(self, commit_data: CommitData):
        new_batch = commit_data.new_batches[0]
        l1_batch_number = new_batch.batchNumber
        status = self._to_text("sent_to_server" if WITH_PROOFS else "unpicked")
        created_at = self._to_ts(new_batch.timestamp)
        updated_at = self._to_ts(new_batch.timestamp)
        prover_taken_at = self._to_ts(new_batch.timestamp)
        cols = """(l1_batch_number, status, created_at, updated_at, prover_taken_at)"""
        vals = f"({l1_batch_number}, {status}, {created_at}, {updated_at}, {prover_taken_at})"
        return f"INSERT INTO proof_generation_details {cols} VALUES {vals};"

    # Table: vm_runner_protective_reads
    #
    #            Column                     |  Type   | Collation | Nullable | Default |
    #---------------------------------------+---------+-----------+----------+---------+
    # l1_batch_number                       | bigint  |           | not null |         |
    # created_at                            | ts      |           | not null |         |
    # updated_at                            | ts      |           | not null |         |
    # time_taken                            | time    |           |          |         |
    # processing_started_at                 | ts      |           |          |         |
    #---------------------------------------+---------+-----------+----------+---------+
    def gen_vm_runner_protective_reads(self, commit_data: CommitData):
        new_batch = commit_data.new_batches[0]
        l1_batch_number = new_batch.batchNumber
        created_at = self._to_ts(new_batch.timestamp)
        updated_at = self._to_ts(new_batch.timestamp)
        time_taken = self._fmt("00:00:00", quote=True)
        processing_started_at = self._to_ts(new_batch.timestamp)
        cols = "(l1_batch_number, created_at, updated_at, time_taken, processing_started_at)"
        vals = f"({l1_batch_number}, {created_at}, {updated_at}, {time_taken}, {processing_started_at})"
        return f"INSERT INTO vm_runner_protective_reads {cols} VALUES {vals};"

    # Table: protective_reads
    #
    #            Column                     |  Type   | Collation | Nullable | Default |
    #---------------------------------------+---------+-----------+----------+---------+
    # l1_batch_number                       | bigint  |           | not null |         |
    # address                               | bytea   |           | not null |         |
    # key                                   | bytea   |           | not null |         |
    # created_at                            | ts      |           | not null |         |
    # updated_at                            | ts      |           | not null |         |
    #---------------------------------------+---------+-----------+----------+---------+
    def gen_protective_reads(self, l1_batch):
        # Not required
        pass


def main(args):
    # Fetch transactions data
    commit_tx = w3.eth.get_transaction(args.commit_tx_hash)
    prove_tx = w3.eth.get_transaction(args.prove_tx_hash)
    execute_tx = w3.eth.get_transaction(args.execute_tx_hash)

    # Read tree
    tree = Tree(args.tree_file)

    # Parse commit calldata
    commit_data = parse_calldata(commit_tx['input'].hex())
    assert commit_data.batch_from == commit_data.batch_to, "Batch from and batch to must be the same"
    batch = commit_data.new_batches[0]
    commit_data.print_summary()
   
    # Parse pubdata input
    with open(args.pubdata_input_file, "r") as f:
        pubdata_input = f.read().strip()
    pubdata = parse_pubdata(pubdata_input)
    pubdata.print_summary()

    # Explorer Data
    batch_details = ExplorerData.get_batch_details(batch.batchNumber, args.explorer_data_dir, args.explorer_data_format)

    # Generate SQL queries
    gen = QueryGenerator()
    with open(args.sql_output, "w") as f:
        f.write("BEGIN;\n\n")

        # eth_txs + eth_txs_history (must be before l1_batches)
        f.write("-- eth_txs + eth_txs_history --\n")
        f.write(gen.gen_eth_txs(commit_tx, EthTxType.COMMIT) + "\n")
        f.write(gen.gen_eth_txs_history(commit_tx) + "\n") # must be after commit eth_txs
        f.write(gen.gen_eth_txs(prove_tx, EthTxType.PROVE) + "\n") # must be after commit eth_ths_history
        f.write(gen.gen_eth_txs_history(prove_tx) + "\n") # must be after prove eth_txs
        f.write(gen.gen_eth_txs(execute_tx, EthTxType.EXECUTE) + "\n") # must be after prove eth_txs_history
        f.write(gen.gen_eth_txs_history(execute_tx) + "\n") # must be after execute eth_txs
        
        # l1_batches
        f.write("-- l1_batches --\n")
        f.write(gen.gen_l1_batches(commit_data, commit_tx, pubdata, batch_details) + "\n")
        
        # l2_to_l1_logs
        f.write("-- l2_to_l1_logs --\n")
        f.write(gen.gen_l2_to_l1_logs(commit_data, pubdata, batch_details) + "\n")

        # factory_deps
        f.write("-- factory_deps --\n")
        f.write(gen.gen_factory_deps(commit_data) + "\n")

        # proof_generation_details (must be after l1_batches)
        f.write("-- proof_generation_details --\n")
        f.write(gen.gen_proof_generation_details(commit_data) + "\n")
        
        # commitments (must be after l1_batches)
        f.write("-- commitments --\n")
        f.write(gen.gen_commitments(commit_data) + "\n")        
        
        # miniblocks
        f.write("-- miniblocks --\n")
        f.write(gen.gen_miniblocks(batch_details) + "\n")

        # transactions
        f.write("-- transactions --\n")
        f.write(gen.gen_transactions(batch_details) + "\n")

        # events
        f.write("-- events --\n")
        f.write(gen.gen_events(batch_details) + "\n")

        # initial_writes
        f.write("-- initial_writes --\n")
        f.write(gen.gen_initial_writes(commit_data, pubdata) + "\n")

        # storage_logs
        f.write("-- storage_logs --\n")
        f.write(gen.gen_storage_logs(commit_data, pubdata, tree, batch_details) + "\n")

        # vm_runner_protective_reads
        f.write("-- vm_runner_protective_reads --\n")
        f.write(gen.gen_vm_runner_protective_reads(commit_data) + "\n")

        if args.tree_file_output:
            tree.save(args.tree_file_output)
            print(f"Updated tree saved to {args.tree_file_output}")

        f.write("\nCOMMIT;\n")

    print(f"SQL queries saved to {args.sql_output}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Generate SQL queries for ZkSync EraVM L1 recovery.")
    parser.add_argument("--rpc", help="L1 RPC URL")
    parser.add_argument("--commit_tx_hash", help="Commit TX Hash")
    parser.add_argument("--prove_tx_hash", help="Prove TX Hash")
    parser.add_argument("--execute_tx_hash", help="Execute TX Hash")
    parser.add_argument("--pubdata_input_file", help="Path to Pubdata Input file")
    parser.add_argument("--tree_file", help="Path to Tree file")
    parser.add_argument("--explorer_data_dir", help="Path to Explorer Data directory")
    parser.add_argument("--explorer_data_format", help="postgres or blockscout")
    parser.add_argument("--tree_file_output", help="Path to output updatedTree file")
    parser.add_argument("--sql_output", required=True, help="File to write the SQL queries to")
    args = parser.parse_args()

    global w3
    w3 = Web3(Web3.HTTPProvider(args.rpc))
    if not w3.is_connected():
        print(f"[!] Failed to connect to RPC: {args.rpc}")
        sys.exit(1)
    
    main(args)
