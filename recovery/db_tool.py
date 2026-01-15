#!/usr/bin/env python3
import psycopg2
import psycopg2.extras
import argparse
import json
import sys
from typing import List, Dict, Tuple, Any, Set

# ==========================================
# Configuration
# ==========================================
SANDBOX_DB_URL = "postgres://postgres:notsecurepassword@localhost:5432/zksync_sandbox"
TARGET_DB_URL = "postgres://postgres:notsecurepassword@localhost:5432/zksync_latest"

# Table Definitions: (Table Name, List of Columns, Primary Key(s))
TABLES = {
    "commitments": {
        "cols": ["l1_batch_number", "events_queue_commitment", "bootloader_initial_content_commitment"],
        "pk": ["l1_batch_number"]
    },
    "eth_txs": {
        "cols": ["id", "nonce", "raw_tx", "contract_address", "tx_type", "gas_used", "created_at", "updated_at", "has_failed", "sent_at_block", "confirmed_eth_tx_history_id", "predicted_gas_cost", "from_addr", "blob_sidecar", "is_gateway", "chain_id", "status"],
        "pk": ["id"]
    },
    "eth_txs_history": {
        "cols": ["id", "eth_tx_id", "tx_hash", "created_at", "updated_at", "base_fee_per_gas", "priority_fee_per_gas", "confirmed_at", "signed_raw_tx", "sent_at_block", "sent_at", "blob_base_fee_per_gas", "max_gas_per_pubdata", "predicted_gas_limit", "sent_successfully", "finality_status"],
        "pk": ["id"]
    },
    "events": {
        "cols": ["miniblock_number", "tx_hash", "tx_index_in_block", "address", "event_index_in_block", "event_index_in_tx", "topic1", "topic2", "topic3", "topic4", "value", "created_at", "updated_at", "tx_initiator_address"],
        "pk": ["miniblock_number", "event_index_in_block"]
    },
    "factory_deps": {
        "cols": ["bytecode_hash", "bytecode", "miniblock_number", "created_at", "updated_at"],
        "pk": ["bytecode_hash"]
    },
    "initial_writes": {
        "cols": ["hashed_key", "l1_batch_number", "created_at", "updated_at", "index"],
        "pk": ["hashed_key"]
    },
    "l1_batches": {
        "cols": ["number", "timestamp", "is_sealed", "l1_tx_count", "l2_tx_count", "bloom", "priority_ops_onchain_data", "hash", "commitment", "eth_prove_tx_id", "eth_commit_tx_id", "eth_execute_tx_id", "created_at", "updated_at", "merkle_root_hash", "l2_to_l1_logs", "l2_to_l1_messages", "predicted_commit_gas_cost", "predicted_prove_gas_cost", "predicted_execute_gas_cost", "initial_bootloader_heap_content", "used_contract_hashes", "compressed_initial_writes", "compressed_repeated_writes", "l2_l1_compressed_messages", "l2_l1_merkle_root", "rollup_last_leaf_index", "zkporter_is_available", "bootloader_code_hash", "default_aa_code_hash", "base_fee_per_gas", "aux_data_hash", "pass_through_data_hash", "meta_parameters_hash", "skip_proof", "l1_gas_price", "l2_fair_gas_price", "protocol_version", "system_logs", "compressed_state_diffs", "storage_refunds", "pubdata_input", "predicted_circuits", "predicted_circuits_by_type", "pubdata_costs", "tree_writes", "fair_pubdata_price", "fee_address", "evm_emulator_code_hash", "state_diff_hash", "aggregation_root", "local_root", "batch_chain_merkle_path", "sealed_at", "final_precommit_eth_tx_id", "pubdata_limit", "blobs_amount", "batch_chain_merkle_path_until_msg_root"],
        "pk": ["number"]
    },
    "l2_to_l1_logs": {
        "cols": ["miniblock_number", "log_index_in_miniblock", "log_index_in_tx", "tx_hash", "shard_id", "is_service", "tx_index_in_miniblock", "tx_index_in_l1_batch", "sender", "key", "value", "created_at", "updated_at"],
        "pk": ["miniblock_number", "log_index_in_miniblock"]
    },
    "miniblocks": {
        "cols": ["number", "l1_batch_number", "timestamp", "hash", "l1_tx_count", "l2_tx_count", "base_fee_per_gas", "gas_per_pubdata_limit", "created_at", "updated_at", "l1_gas_price", "l2_fair_gas_price", "bootloader_code_hash", "default_aa_code_hash", "protocol_version", "virtual_blocks", "fee_account_address", "fair_pubdata_price", "gas_limit", "logs_bloom", "evm_emulator_code_hash", "l2_da_validator_address", "pubdata_type", "rolling_txs_hash", "eth_precommit_tx_id"],
        "pk": ["number"]
    },
    "proof_generation_details": {
        "cols": ["l1_batch_number", "status", "proof_gen_data_blob_url", "proof_blob_url", "created_at", "updated_at", "prover_taken_at", "vm_run_data_blob_url", "proving_mode"],
        "pk": ["l1_batch_number"]
    },
    "storage_logs": {
        "cols": ["hashed_key", "address", "key", "value", "operation_number", "tx_hash", "miniblock_number", "created_at", "updated_at"],
        "pk": ["hashed_key", "miniblock_number", "operation_number"]
    },
    "transactions": {
        "cols": ["hash", "is_priority", "full_fee", "layer_2_tip_fee", "initiator_address", "nonce", "signature", "input", "data", "received_at", "priority_op_id", "l1_batch_number", "index_in_block", "error", "gas_limit", "gas_per_storage_limit", "gas_per_pubdata_limit", "tx_format", "created_at", "updated_at", "execution_info", "contract_address", "in_mempool", "l1_block_number", "value", "paymaster", "paymaster_input", "max_fee_per_gas", "max_priority_fee_per_gas", "effective_gas_price", "miniblock_number", "l1_batch_tx_index", "refunded_gas", "l1_tx_mint", "l1_tx_refund_recipient", "upgrade_id", "timestamp_asserter_range_start", "timestamp_asserter_range_end"],
        "pk": ["hash"]
    },
    "vm_runner_protective_reads": {
        "cols": ["l1_batch_number", "created_at", "updated_at", "time_taken", "processing_started_at"],
        "pk": ["l1_batch_number"]
    },
    # "protective_reads": {
    #     "cols": ["l1_batch_number", "address", "key", "created_at", "updated_at"],
    #     "pk": ["address", "key", "l1_batch_number"]
    # }    
}

# ==========================================
# Helpers
# ==========================================

def get_db_connection(url):
    try:
        conn = psycopg2.connect(url)
        return conn
    except Exception as e:
        print(f"Error connecting to DB {url}: {e}")
        sys.exit(1)

def quote_ident(name):
    """Simple identifier quoting."""
    return f'"{name}"'

def fetch_table_data(conn, table_name, columns, pk_cols):
    """
    Fetches all data from a table, returning a dict keyed by primary key tuple.
    """
    cols_str = ", ".join([quote_ident(c) for c in columns])
    query = f"SELECT {cols_str} FROM {quote_ident(table_name)}"
    
    cur = conn.cursor(cursor_factory=psycopg2.extras.DictCursor)
    cur.execute(query)
    rows = cur.fetchall()
    cur.close()

    data_map = {}
    for row in rows:
        # Create a tuple for the primary key
        pk_val = tuple(row[k] for k in pk_cols)
        data_map[pk_val] = dict(row)
    
    return data_map

def values_are_equal(v1, v2):
    """Helper to compare values, handling bytes/memoryview/None nuances."""
    if isinstance(v1, memoryview):
        v1 = bytes(v1)
    if isinstance(v2, memoryview):
        v2 = bytes(v2)
    return v1 == v2

# ==========================================
# Compare Content
# ==========================================

def compare_content(table_filter=None, col_filter=None, verbose=False):
    print(">>> Connecting to databases...")
    sb_conn = get_db_connection(SANDBOX_DB_URL)
    tg_conn = get_db_connection(TARGET_DB_URL)
    
    print("\n>>> Starting Comparison...")

    # Determine which tables to check
    tables_to_check = TABLES.keys()
    if table_filter:
        if table_filter not in TABLES:
            print(f"Error: Table '{table_filter}' not found.")
            return
        tables_to_check = [table_filter]    

    for table_name in tables_to_check:
        meta = TABLES[table_name]
        pk_cols = meta["pk"]
        cols = meta["cols"]

        if col_filter:
            if col_filter not in cols:
                print(f"Error: Column '{col_filter}' not found in table '{table_name}'.")
                continue
            cols_to_compare = [col_filter]
        else:
            cols_to_compare = cols
        
        print(f"\n--- Comparing Table: {table_name} ---")
        if col_filter:
            print(f"    (column: {col_filter})")

        # 1. Check Row Counts
        sb_cur = sb_conn.cursor(); tg_cur = tg_conn.cursor()
        sb_cur.execute(f"SELECT COUNT(*) FROM {quote_ident(table_name)}")
        tg_cur.execute(f"SELECT COUNT(*) FROM {quote_ident(table_name)}")
        sb_count = sb_cur.fetchone()[0]
        tg_count = tg_cur.fetchone()[0]
        sb_cur.close(); tg_cur.close()

        if sb_count != tg_count:
            print(f"❌ Row count mismatch! Sandbox: {sb_count}, Target: {tg_count}")
            continue
        else:
            print(f"✅ Row count matches: {sb_count}")

        # Fetch Data for deep inspection
        # Note: This loads table into memory. Not recommended for huge tables.
        try:
            sb_data = fetch_table_data(sb_conn, table_name, cols, pk_cols)
            tg_data = fetch_table_data(tg_conn, table_name, cols, pk_cols)
        except Exception as e:
            print(f"⚠️ Error fetching data for {table_name}: {e}\n")
            continue

        # 2. Check Primary Keys Set
        sb_keys = set(sb_data.keys())
        tg_keys = set(tg_data.keys())

        if sb_keys != tg_keys:
            print("❌ Primary Key mismatch!")
            diff_sb = sb_keys - tg_keys
            diff_tg = tg_keys - sb_keys
            if diff_sb:
                print(f"   Keys in Sandbox but not Target ({len(diff_sb)}): {list(diff_sb)[:5]}...")
            if diff_tg:
                print(f"   Keys in Target but not Sandbox ({len(diff_tg)}): {list(diff_tg)[:5]}...")
            print(f"   Skipping deep inspection for {table_name}.\n")
            continue
        else:
            print("✅ Primary Key sets match.")

        # 3. Column by Column Comparison
        mismatches = {col: 0 for col in cols}

        for pk, sb_row in sb_data.items():
            tg_row = tg_data[pk]
            for col in cols_to_compare:
                if not values_are_equal(sb_row[col], tg_row[col]):
                    mismatches[col] += 1
                    if verbose and mismatches[col] <= 5:
                        print(f"   [Diff] PK {pk} Col '{col}': Sandbox='{sb_row[col]}' vs Target='{tg_row[col]}'")

        has_diff = False
        for col, count in mismatches.items():
            if count > 0:
                has_diff = True
                print(f"❌ Column '{col}': {count} mismatching rows")
        
        if not has_diff:
            print("✅ All columns match perfectly.")

    sb_conn.close()
    tg_conn.close()

# ==========================================
# Copy Content
# ==========================================

def copy_content(table_name, column_name=None):
    if table_name not in TABLES:
        print(f"Error: Table '{table_name}' not defined in script.")
        return

    print(">>> Connecting to databases...")
    sb_conn = get_db_connection(SANDBOX_DB_URL)
    tg_conn = get_db_connection(TARGET_DB_URL)
    
    meta = TABLES[table_name]
    pk_cols = meta["pk"]
    all_cols = meta["cols"]

    if column_name:
        # --- Update Specific Column ---
        if column_name not in all_cols:
            print(f"Error: Column '{column_name}' does not exist in table '{table_name}'")
            return
        
        print(f">>> Updating column '{column_name}' in '{table_name}' (Sandbox) from Target...")
        
        # 1. Fetch PKs and Target Values
        cols_to_fetch = pk_cols + [column_name]
        cols_str = ", ".join([quote_ident(c) for c in cols_to_fetch])
        
        tg_cur = tg_conn.cursor()
        tg_cur.execute(f"SELECT {cols_str} FROM {quote_ident(table_name)}")
        tg_rows = tg_cur.fetchall()
        tg_cur.close()

        if not tg_rows:
            print("Target table is empty. Nothing to update.")
            return

        sb_cur = sb_conn.cursor()
        
        # 2. Update Row by Row
        # Note: Ideally usage of executemany is better, but this ensures strict PK matching logic requested
        updated_count = 0
        try:
            for row in tg_rows:
                # row is a tuple, indices match cols_to_fetch order
                # PK values are at the start
                pk_vals = row[:len(pk_cols)]
                new_val = row[len(pk_cols)]

                if isinstance(new_val, (dict, list)):
                    new_val = json.dumps(new_val)                
                
                # Build WHERE clause for PK
                where_clause = " AND ".join([f"{quote_ident(k)} = %s" for k in pk_cols])
                
                query = f"UPDATE {quote_ident(table_name)} SET {quote_ident(column_name)} = %s WHERE {where_clause}"
                
                # Execute update
                sb_cur.execute(query, (new_val,) + pk_vals)
                updated_count += sb_cur.rowcount
            
            sb_conn.commit()
            print(f"✅ Successfully updated {updated_count} rows in '{table_name}' column '{column_name}'.")

        except Exception as e:
            sb_conn.rollback()
            print(f"❌ Error updating column: {e}")

    else:
        # --- Copy Entire Table ---
        print(f">>> Full Copy: Replacing table '{table_name}' in Sandbox with Target data...")
        
        # 1. Fetch All Data from Target
        tg_cur = tg_conn.cursor()
        cols_str = ", ".join([quote_ident(c) for c in all_cols])
        tg_cur.execute(f"SELECT {cols_str} FROM {quote_ident(table_name)}")
        tg_rows = tg_cur.fetchall()
        tg_cur.close()
        
        sb_cur = sb_conn.cursor()

        try:
            # 2. Delete All Data in Sandbox Table
            print(f"   Deleting existing data in sandbox '{table_name}'...")
            sb_cur.execute(f"TRUNCATE TABLE {quote_ident(table_name)} CASCADE")
            
            # 3. Insert Target Data
            if tg_rows:
                print(f"   Inserting {len(tg_rows)} rows...")

                processed_rows = []
                for row in tg_rows:
                    new_row = []
                    for val in row:
                        if isinstance(val, (dict, list)):
                            new_row.append(json.dumps(val))
                        else:
                            new_row.append(val)
                    processed_rows.append(tuple(new_row))

                placeholders = ", ".join(["%s"] * len(all_cols))
                insert_query = f"INSERT INTO {quote_ident(table_name)} ({cols_str}) VALUES ({placeholders})"
                
                psycopg2.extras.execute_batch(sb_cur, insert_query, processed_rows)
            
            sb_conn.commit()
            print(f"✅ Successfully replaced table '{table_name}' with {len(tg_rows)} rows.")

        except Exception as e:
            sb_conn.rollback()
            print(f"❌ Error during full copy: {e}")

    sb_conn.close()
    tg_conn.close()


# ==========================================
# Main CLI
# ==========================================

def main():
    parser = argparse.ArgumentParser(description="DB Tool for checking and syncing ZKSync Sandbox DB")
    subparsers = parser.add_subparsers(dest="command", help="Command to run")

    # Command: compare
    compare_parser = subparsers.add_parser("compare", help="Compare sandbox against target")
    compare_parser.add_argument("table", nargs="?", help="Specific table to compare (optional)")
    compare_parser.add_argument("--col", help="Specific column to compare (optional)")
    compare_parser.add_argument("-v", "--verbose", action="store_true", help="Print diff details")

    # Command: copy
    copy_parser = subparsers.add_parser("copy", help="Copy content from target to sandbox")
    copy_parser.add_argument("table", help="Table name to copy")
    copy_parser.add_argument("--col", help="Specific column to update")

    args = parser.parse_args()

    if args.command == "compare":
        compare_content(table_filter=args.table, col_filter=args.col, verbose=args.verbose)
    elif args.command == "copy":
        copy_content(args.table, args.col)
    else:
        parser.print_help()

if __name__ == "__main__":
    main()
