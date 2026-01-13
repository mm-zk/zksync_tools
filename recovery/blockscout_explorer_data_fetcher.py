import requests
import json
import time
import os
import threading
from concurrent.futures import ThreadPoolExecutor, as_completed

# ==========================================
# CONFIGURATION
# ==========================================
BATCH_ID = "1772"
REQUESTS_PER_SEC = 5
MAX_WORKERS = 10

API_BASE = "https://zero-network.calderaexplorer.xyz/api/v2"

# Output Configuration
OUTPUT_DIR = f"zero/explorer/batches/{BATCH_ID}"
DIRS = {
    "pages": os.path.join(OUTPUT_DIR, "pages"),
    "txs": os.path.join(OUTPUT_DIR, "txs"),
    "logs": os.path.join(OUTPUT_DIR, "logs"),
    "blocks": os.path.join(OUTPUT_DIR, "blocks"),
    # "traces": os.path.join(OUTPUT_DIR, "traces"),
}

# Ensure directories exist
if not os.path.exists(OUTPUT_DIR):
    os.makedirs(OUTPUT_DIR)
for path in DIRS.values():
    os.makedirs(path, exist_ok=True)

# ==========================================
# RATE LIMITER
# ==========================================
class RateLimiter:
    def __init__(self, max_per_second):
        self.interval = 1.0 / max_per_second
        self.lock = threading.Lock()
        self.last_call = 0

    def wait_for_slot(self):
        with self.lock:
            now = time.time()
            elapsed = now - self.last_call
            if elapsed < self.interval:
                time.sleep(self.interval - elapsed)
            self.last_call = time.time()

limiter = RateLimiter(REQUESTS_PER_SEC)

# ==========================================
# GENERIC FETCHER
# ==========================================
def get_data_with_cache(url, filepath, params=None):
    # 1. Try Cache
    if os.path.exists(filepath):
        try:
            with open(filepath, 'r', encoding='utf-8') as f:
                return json.load(f), True
        except json.JSONDecodeError:
            pass 

    # 2. Rate Limit
    limiter.wait_for_slot()

    # 3. Network Request
    try:
        response = requests.get(url, params=params)
        response.raise_for_status()
        data = response.json()
        
        with open(filepath, 'w', encoding='utf-8') as f:
            json.dump(data, f, indent=2)
            
        return data, False
    except Exception as e:
        # Don't print 404s for N-1 checks if it's genesis
        if "404" not in str(e):
            print(f"[!] Error fetching {url}: {e}")
        return None, False

# ==========================================
# WORKER TASKS
# ==========================================
def fetch_block_task(block_num):
    url = f"{API_BASE}/blocks/{block_num}"
    filepath = os.path.join(DIRS["blocks"], f"{block_num}.json")
    _, cached = get_data_with_cache(url, filepath)
    return f"Block {block_num}", cached

def fetch_tx_task(category, tx_hash, url_suffix=""):
    if url_suffix:
        url = f"{API_BASE}/transactions/{tx_hash}/{url_suffix}"
    else:
        url = f"{API_BASE}/transactions/{tx_hash}"
        
    filepath = os.path.join(DIRS[category], f"{tx_hash}.json")
    _, cached = get_data_with_cache(url, filepath)
    return f"Tx {tx_hash[:6]}.. {category}", cached

# ==========================================
# MAIN LOGIC
# ==========================================
def determine_block_range():
    print(f"[*] Calculating Block Range for Batch {BATCH_ID}...")
    
    # 1. Fetch Current Batch (N)
    curr_url = f"{API_BASE}/zksync/batches/{BATCH_ID}"
    curr_file = os.path.join(OUTPUT_DIR, "details.json")
    curr_data, _ = get_data_with_cache(curr_url, curr_file)
    
    if not curr_data:
        print("[!] Could not fetch current batch info.")
        return None
    
    end_block_inclusive = int(curr_data.get('end_block'))

    # 2. Fetch Previous Batch (N-1)
    prev_id = int(BATCH_ID) - 1
    start_block = 0 # Default for genesis/first batch
    
    if prev_id >= 0:
        prev_url = f"{API_BASE}/zksync/batches/{prev_id}"
        # We save N-1 summary just for caching purposes
        prev_file = os.path.join(OUTPUT_DIR, f"prev_batch_{prev_id}_summary.json")
        prev_data, _ = get_data_with_cache(prev_url, prev_file)
        
        if prev_data:
            # Range starts after the previous batch's end block
            start_block = int(prev_data.get('end_block')) + 1
            
    print(f"    Batch {BATCH_ID} Range: Blocks {start_block} -> {end_block_inclusive}")
    return range(start_block, end_block_inclusive + 1)

def main():
    print(f"[*] Output Directory: {OUTPUT_DIR}")
    
    # --- PHASE 1: FETCH BLOCKS ---
    block_range = determine_block_range()
    
    if block_range:
        print(f"\n--- Phase 1: Fetching {len(block_range)} Blocks ---")
        with ThreadPoolExecutor(max_workers=MAX_WORKERS) as executor:
            futures = [executor.submit(fetch_block_task, b) for b in block_range]
            
            count = 0
            for future in as_completed(futures):
                desc, cached = future.result()
                count += 1
                status = "Cache" if cached else "Net"
                print(f"    [{count}/{len(block_range)}] {desc} ({status})", end='\r')
        print("\n    Blocks done.")
    else:
        print("[!] Skipping blocks due to error.")

    # --- PHASE 2: FETCH TRANSACTIONS ---
    print(f"\n--- Phase 2: Fetching Transactions ---")
    current_params = {}
    page_num = 1
    
    with ThreadPoolExecutor(max_workers=MAX_WORKERS) as executor:
        while True:
            # 1. Get Page List (Sequential)
            page_file = os.path.join(DIRS["pages"], f"page_{page_num}.json")
            batch_txs_url = f"{API_BASE}/transactions/zksync-batch/{BATCH_ID}"
            
            page_data, _ = get_data_with_cache(batch_txs_url, page_file, params=current_params)
            
            if not page_data: break

            items = page_data.get('items', [])
            if not items:
                print(f"    Page {page_num}: Empty")
            
            # 2. Queue Tx Tasks (Parallel)
            futures = []
            print(f"    Page {page_num}: Queuing {len(items)*3} tasks...")
            
            for item in items:
                tx_hash = item.get('hash')
                if not tx_hash: continue
                
                futures.append(executor.submit(fetch_tx_task, "txs", tx_hash, ""))
                futures.append(executor.submit(fetch_tx_task, "logs", tx_hash, "logs"))
                # futures.append(executor.submit(fetch_tx_task, "traces", tx_hash, "raw-trace"))

            # 3. Wait for Page Completion
            done_count = 0
            for future in as_completed(futures):
                desc, cached = future.result()
                done_count += 1
                status = "Cache" if cached else "RPC"
                # Overwrite line for cleaner output
                print(f"    [{done_count}/{len(futures)}] {desc} ({status})".ljust(100), end='\r')
            print("")

            # 4. Pagination
            next_params = page_data.get('next_page_params')
            if next_params:
                current_params = next_params
                page_num += 1
            else:
                print("\n[*] Job complete.")
                break

if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\n[!] Stopped by user.")
