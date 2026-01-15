import json
import psycopg2
import psycopg2.extras
import argparse
import sys
from typing import List, Dict, Any

# Assumption: the database doesn't contain any information about "open" batches:
# All initial_writes and storage_logs items belong to the batches that are already
# committed to L1, and don't need to be recovered.

# ==========================================
# Helpers
# ==========================================
class HexJsonEncoder(json.JSONEncoder):
    """Encodes bytes objects as 0x hex strings for JSON output."""
    def default(self, obj):
        if isinstance(obj, (bytes, bytearray, memoryview)):
            return "0x" + bytes(obj).hex()
        return super().default(obj)

def get_hashed_keys_from_indices(cur, indices: List[int]) -> Dict[str, int]:
    query = f"""SELECT index, hashed_key FROM initial_writes"""
    if indices:
        # Always include the max index
        indices_vals = ["(SELECT MAX(index) FROM initial_writes)"]
        indices_vals.extend(indices)
        indices_str = ",".join(map(str, indices_vals))
        query += f" WHERE index IN ({indices_str})"
    
    print(f"[1/3] Fetching hashed_keys for {len(indices) if indices else 'all'} indices...")
    cur.execute(query)
    rows = cur.fetchall()

    # Create a map: { hashed_key_hex_string : index }
    mapping = {}
    for idx, hashed_key_bytes in rows:
        hashed_key_hex = f"0x{bytes(hashed_key_bytes).hex()}"
        mapping[hashed_key_hex] = idx
        
    print(f"      Found {len(mapping)} matching keys.")
    return mapping

def get_latest_storage_logs(cur, hashed_keys_hex: List[str]) -> List[Dict]:
    if not hashed_keys_hex:
        return []

    print(f"[2/3] Fetching latest storage logs for {len(hashed_keys_hex)} keys...")
    values_list = []
    for hk in hashed_keys_hex:
        values_list.append(f"('\\x{hk[2:]}'::bytea)")
    
    values_clause = ",\n".join(values_list)
    query = f"""
        WITH target_keys (h_key) AS (
            VALUES 
            {values_clause}
        )
        SELECT 
            res.hashed_key, 
            res.address, 
            res.key, 
            res.value
        FROM target_keys t
        CROSS JOIN LATERAL (
            SELECT hashed_key, address, key, value
            FROM storage_logs s
            WHERE s.hashed_key = t.h_key
            ORDER BY s.miniblock_number DESC, s.operation_number DESC
            LIMIT 1
        ) res;
    """
    cur.execute(query)
    return cur.fetchall()

def main(args):
    # Load Indices
    indices = None
    if args.indices:
        try:
            with open(args.indices, 'r') as f:
                indices = json.load(f)
                if not isinstance(indices, list):
                    raise ValueError("Input JSON must be a list of integers")
                indices = [int(x) for x in indices]
                print(f"Loaded {len(indices)} indices from {args.indices}")
        except FileNotFoundError:
            print(f"Error: File {args.indices} not found.")
            sys.exit(1)
    else:
        print("No indices provided. Will use all indices from the database.")

    # Connect to DB
    try:
        conn = psycopg2.connect(dsn=args.db_url)
        cur = conn.cursor()
    except Exception as e:
        print(f"Database connection error: {e}")
        sys.exit(1)

    try:
        # Get map of { hashed_key : index }
        indices_by_hashed_key = get_hashed_keys_from_indices(cur, indices)

        if not indices_by_hashed_key:
            print("No hashed keys found for these indices.")
            sys.exit(0)

        # Query storage logs using the keys we just found
        hashed_keys = list(indices_by_hashed_key.keys())
        storage_rows = get_latest_storage_logs(cur, hashed_keys)

        # Combine Data
        print(f"[3/3] Combining data and saving to {args.output}...")
        
        tree = {}
        for row in storage_rows:
            hashed_key = "0x" + bytes(row[0]).hex()
            address = f"0x{bytes(row[1]).hex()}" if row[1] is not None else None
            key = f"0x{bytes(row[2]).hex()}" if row[2] is not None else None
            value = f"0x{bytes(row[3]).hex()}"
            # Retrieve the index
            index = indices_by_hashed_key.get(hashed_key)
            tree[hashed_key] = {
                "address": address,
                "key": key,
                "value": value,
                "index": index
            }
        # Save to File
        with open(args.output, 'w') as f:
            sorted_data = dict(sorted(tree.items(), key=lambda x: int(x[1]['index'])))
            json.dump(sorted_data, f, indent=2, sort_keys=False)
        print("Success.")

    except Exception as e:
        print(f"Error during execution: {e}")
    finally:
        cur.close()
        conn.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--db_url", 
        default="postgres://postgres:notsecurepassword@localhost:5432/zksync_sandbox",
        help="PostgreSQL connection string (URI)"
    )
    parser.add_argument("--indices", help="Path to input indices JSON")
    parser.add_argument("--output", default="tree.json", help="Path to output JSON")
    args = parser.parse_args()
    main(args)
