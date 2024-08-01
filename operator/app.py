from flask import Flask, render_template, abort
from flask_caching import Cache
from web3 import Web3
from utils import get_main_contract, get_batch_details, get_chain_id
from calldata_utils import parse_commitcall_calldata
from system_storage import get_system_context_state, get_l1_state_storage
from shared_bridge_storage import get_chain_balance_info
import json
import requests
import os


app = Flask(__name__)
config_file = os.environ.get("CONFIG_FILE") or "operator/config.json"
with open(config_file,'r') as config_file:
    config_data = json.load(config_file)


def get_bridgehub_contract(l2_url):
    headers = {"Content-Type": "application/json"}
    data = {"jsonrpc": "2.0", "id": 1, "method": "zks_getBridgehubContract", "params": []}
    response = requests.post(l2_url, headers=headers, data=json.dumps(data))
    return Web3.to_checksum_address(response.json()["result"])


def detect_bridgehub(chains):
    for chain in chains:
        if "l2_url" in chains[chain]:
            return get_bridgehub_contract(chains[chain]["l2_url"])
    return None


def autodetect_config(config_data):
    full_config = config_data["networks"]
    for network in full_config:
        for shared_bridge in full_config[network]["shared_bridges"]:
            if "bridgehub" not in full_config[network]["shared_bridges"][shared_bridge]:
                config_data["networks"][network]["shared_bridges"][shared_bridge]["bridgehub"] = detect_bridgehub(config_data["networks"][network]["shared_bridges"][shared_bridge]["chains"])
                


autodetect_config(config_data)
app.config.update(config_data)


cache = Cache(app, config={'CACHE_TYPE': 'simple'})

def format_int(value):
    if not isinstance(value, int):
        return value  # Optionally, handle non-integer inputs
    reversed_str = str(value)[::-1]
    formatted_str = '_'.join(reversed_str[i:i+3] for i in range(0, len(reversed_str), 3))
    return formatted_str[::-1]

def format_eth(value):
    if not isinstance(value, int):
        return value  # Optionally, handle non-integer inputs
    
    return round(value / 10**18, 2)

def format_protocol_version(value):
    if not isinstance(value, int):
        return value
    # old protocols
    if value < 25:
        return value
    return f"{value >> 32}.{value%(1<<32)}"

def remove_leading_zeros_hex(hex_str):
    # Check if the input is a string
    if not isinstance(hex_str, str):
        return hex_str  # Optionally, handle non-string inputs

    # Remove leading zeros while keeping the '0x' prefix
    cleaned_hex_str = '0x' + hex_str.lstrip("0x").lstrip("0")

    # If the string becomes only '0x', it means the original number was 0
    if cleaned_hex_str == '0x':
        cleaned_hex_str = '0x0'

    return cleaned_hex_str


app.jinja_env.filters['format_int'] = format_int
app.jinja_env.filters['format_eth'] = format_eth

app.jinja_env.filters['remove_leading_zeros_hex'] = remove_leading_zeros_hex
app.jinja_env.filters['format_protocol_version'] = format_protocol_version

MEMOISE_DURATION=1



@app.route('/bridge/<l1_network>/<l2_network>')
@cache.memoize(MEMOISE_DURATION)
def bridge(l1_network, l2_network):    
    (l1, l2) = get_single_bridge_details(l1_network, l2_network)
    return render_template('single_bridge.html', l2=l2, l1=l1)

@app.route('/shared_bridge/<l1_network>/<l2_network>')
@cache.memoize(MEMOISE_DURATION)
def shared_bridge(l1_network, l2_network):    
    return render_template('shared_bridge.html', data=get_shared_bridge_details(l1_network, l2_network))


@app.route('/')
@cache.memoize(MEMOISE_DURATION)
def box():
    full_config = app.config["networks"]
    for network in full_config:
        full_config[network]["chain_id"] = get_chain_id(full_config[network]["l1_url"])
        for bridge in full_config[network]["single_bridges"]:
            proxy_contract = get_main_contract(full_config[network]["single_bridges"][bridge]["l2_url"])
            chain_id = get_chain_id(full_config[network]["single_bridges"][bridge]["l2_url"])
            full_config[network]["single_bridges"][bridge]["proxy"] = proxy_contract
            full_config[network]["single_bridges"][bridge]["chain_id"] = chain_id

        for shared_bridge in full_config[network]["shared_bridges"]:
            for chain in full_config[network]["shared_bridges"][shared_bridge]["chains"]:
                subchain_info = get_shared_bridge_chain_info(network,
                                                             full_config[network]["shared_bridges"][shared_bridge]["bridgehub"],
                                                             full_config[network]["shared_bridges"][shared_bridge]["chains"][chain]["chain_id"])
                full_config[network]["shared_bridges"][shared_bridge]["chains"][chain]["details"] = subchain_info
    
    return render_template('box.html', networks=full_config)

@app.route('/system')
def system():
    block_id = 24279081
    L2_URL = 'https://mainnet.era.zksync.io'
    system_status = {
        'block_id': block_id,
        'system_context': get_system_context_state(zksync_url=L2_URL, block=block_id)
    }


    return render_template('system.html', system_status=system_status)



@app.route('/batch/<l1_network>/<l2_network>/<int:batch_id>')
@cache.memoize(MEMOISE_DURATION)
def batch(l1_network, l2_network, batch_id):
    l1_config = app.config["networks"][l1_network]
    l2_config = l1_config["single_bridges"][l2_network]

    # Example function to generate text and data based on the ID
    batch = {
        'id': batch_id
    }

    batch_details = get_batch_details(l2_config["l2_url"], batch_id)
    if batch_details is None:
        return "Batch not found", 500
    if batch_details['commitTxHash'] is None:
        return "Batch not committed to L1 yet", 500
        

    batch['commitTxHash'] = batch_details['commitTxHash']

    ethweb3 = Web3(Web3.HTTPProvider(l1_config["l1_url"]))
    # Check if connected successfully
    if not ethweb3.is_connected():
        print("Failed to connect to zkSync node.")
        raise

    try:
        commit_tx = ethweb3.eth.get_transaction(batch['commitTxHash'])
    except Exception as e:
        print(f"An error occurred: {e}")
        raise

    (new_state_root, pubdata_info, parsed_system_logs, pubdata_length, chain_id) = parse_commitcall_calldata(commit_tx['input'], batch_id)

    batch['newStateRoot'] = new_state_root.hex()

    batch['l1_l2_msg_counter'] = pubdata_info[0]
    batch['large_msg_counter'] = pubdata_info[1]
    batch['bytecodes'] = pubdata_info[2]
    batch['initial_writes'] = {k.hex():(v[0], v[1].hex()) for k,v in pubdata_info[3].items()}
    
    batch['repeated_writes'] = {k.hex():(v[0], v[1].hex()) for k,v in pubdata_info[4].items()}

    batch['initial_writes_count'] = len(pubdata_info[3])
    batch['repeated_writes_count'] = len(pubdata_info[4])
    batch['parsed_system_logs'] = parsed_system_logs
    batch['pubdata_length'] = pubdata_length
    batch['pubdata_msg_length'] = pubdata_info[5][0]
    batch['pubdata_bytecode_length'] = pubdata_info[5][1]
    batch['pubdata_statediff_length'] = pubdata_info[5][2]
    uncompressed = len(batch['initial_writes']) * 64 + len(batch["repeated_writes"]) * 40 
    if uncompressed > 0:
        batch['statediff_compression_percent'] = round((batch['pubdata_statediff_length']  * 100 / uncompressed))


    return render_template('batch.html', batch=batch, data = {
        "explorer_tx_prefix":  l1_config['explorer_prefix'] + "tx/"
    })

@app.route('/batch_shared/<l1_network>/<shared_bridge>/<chain_name>/<int:batch_id>')
@cache.memoize(MEMOISE_DURATION)
def shared_bridge_batch(l1_network, shared_bridge, chain_name, batch_id):
    l1_config = app.config["networks"][l1_network]
    shared_bridge_config = l1_config["shared_bridges"][shared_bridge]

    # Example function to generate text and data based on the ID
    batch = {
        'id': batch_id
    }

    batch_details = get_batch_details(shared_bridge_config["chains"][chain_name]["l2_url"], batch_id)
    if batch_details is None:
        return "Batch not found", 500
    if batch_details['commitTxHash'] is None:
        return "Batch not committed to L1 yet", 500
        

    batch['commitTxHash'] = batch_details['commitTxHash']
    print(f"Commit tx hash is {batch['commitTxHash']}")

    ethweb3 = Web3(Web3.HTTPProvider(l1_config["l1_url"]))
    # Check if connected successfully
    if not ethweb3.is_connected():
        print("Failed to connect to zkSync node.")
        raise

    try:
        commit_tx = ethweb3.eth.get_transaction(batch['commitTxHash'])
    except Exception as e:
        print(f"An error occurred: {e}")
        raise

    # TODO: currently pubdata parsing doesn't work - as we put the pubdata into the blobs.
    (new_state_root, pubdata_info, parsed_system_logs, pubdata_length, chain_id) = parse_commitcall_calldata(commit_tx['input'], batch_id)

    batch['newStateRoot'] = new_state_root.hex()
    batch['chainId'] = chain_id

    batch['l1_l2_msg_counter'] = pubdata_info[0]
    batch['large_msg_counter'] = pubdata_info[1]
    batch['bytecodes'] = pubdata_info[2]
    batch['initial_writes'] = {k.hex():(v[0], v[1].hex()) for k,v in pubdata_info[3].items()}
    
    batch['repeated_writes'] = {k.hex():(v[0], v[1].hex()) for k,v in pubdata_info[4].items()}

    batch['initial_writes_count'] = len(pubdata_info[3])
    batch['repeated_writes_count'] = len(pubdata_info[4])
    batch['parsed_system_logs'] = parsed_system_logs
    batch['pubdata_length'] = pubdata_length
    batch['pubdata_msg_length'] = pubdata_info[5][0]
    batch['pubdata_bytecode_length'] = pubdata_info[5][1]
    batch['pubdata_statediff_length'] = pubdata_info[5][2]
    uncompressed = len(batch['initial_writes']) * 64 + len(batch["repeated_writes"]) * 40 
    if uncompressed > 0:
        batch['statediff_compression_percent'] = round((batch['pubdata_statediff_length']  * 100 / uncompressed))


    return render_template('batch.html', batch=batch, data = {
        "explorer_tx_prefix":  l1_config['explorer_prefix'] + "tx/"
    })



def get_shared_bridge_chain_info(l1_network, bridgehub, chain_id):
    l1_config = app.config["networks"][l1_network]
    ethweb3 = Web3(Web3.HTTPProvider(l1_config["l1_url"]))
    # Check if connected successfully
    if not ethweb3.is_connected():
        print("Failed to connect to L1 node.")
        raise Exception("Failed to connect to l1 node")


    bridgehub_abi = [
        {
            "name": "stateTransitionManager",
            "inputs": [
                {
                    "type": "uint256"
                }
            ],
            "outputs": [
                {
                    "type": "address"
                }
            ],
            "type": "function"
        },
        {
            "name": "baseToken",
            "inputs": [
                {
                    "type": "uint256"
                }
            ],
            "outputs": [
                {
                    "type": "address"
                }
            ],
            "type": "function"
        },
        {
            "name": "sharedBridge",
            "inputs": [],
            "outputs": [
                {
                    "type": "address"
                }
            ],
            "type": "function"
        },
    ]

    bridgehub_contract = ethweb3.eth.contract(address=bridgehub, abi=bridgehub_abi)

    basic_info = {
        'state_transition_manager': bridgehub_contract.functions.stateTransitionManager(int(chain_id, 16)).call(),
        'base_token': bridgehub_contract.functions.baseToken(int(chain_id, 16)).call(),
        'base_token_bridge': bridgehub_contract.functions.sharedBridge().call()
    }

    stm_abi = [
        {
            "name": "getHyperchain",
            "inputs": [
                {
                    "type": "uint256"
                }
            ],
            "outputs": [
                {
                    "type": "address"
                }
            ],
            "type": "function"
        },
    ]

    stm_contract = ethweb3.eth.contract(address=basic_info['state_transition_manager'], abi=stm_abi)


    basic_info["state_transition"] = stm_contract.functions.getHyperchain(int(chain_id, 16)).call()

    st_abi = [
        {
            "name": "getPubdataPricingMode",
            "inputs": [
            ],
            "outputs": [
                {
                    "type": "uint256"
                }
            ],
            "type": "function"
        },
    ]

    st_contract = ethweb3.eth.contract(address=basic_info['state_transition'], abi=st_abi)
    basic_info['pubdata_pricing_mode'] = st_contract.functions.getPubdataPricingMode().call()
    
    basic_info['pubdata_pricing_mode_str'] = pricing_mode(basic_info['pubdata_pricing_mode'])
    

    basic_info['balance'] = get_chain_balance_info(l1_config["l1_url"], basic_info['base_token_bridge'], chain_id, basic_info['base_token'])

    return basic_info


def pricing_mode(mode):
    if mode == 0:
        return "Rollup"
    if mode == 1:
        return "Validium"
    
    return f"Unknown mode {mode}"

def get_l2_balance(web3):
    l2_ether_contract_abi = [
        {
            "name": "totalSupply",
            "inputs": [],
            "outputs": [
                {
                    "type": "uint256"
                }
            ],
            "type": "function"
        },
    ]

    l2_ether_contract = web3.eth.contract(address=Web3.to_checksum_address("0x000000000000000000000000000000000000800a"), abi=l2_ether_contract_abi)
    balance = l2_ether_contract.functions.totalSupply().call()
    balance_in_ether = Web3.from_wei(balance, 'ether')

    return (balance, balance_in_ether)



def get_shared_bridge_chain_id_details(l1_network, l2_config, chain_name):
    config = l2_config["chains"][chain_name]
    basic_info = get_shared_bridge_chain_info(l1_network, l2_config["bridgehub"], config["chain_id"])


    web3 = Web3(Web3.HTTPProvider(config["l2_url"]))
    # Check if connected successfully
    if not web3.is_connected():
        print("Failed to connect to L2 node.")
        basic_info['l2_balance'] = 0
        basic_info['l2_gas_price'] = 0
    else:
        (basic_info['l2_balance'], _) = get_l2_balance(web3)
        basic_info['l2_gas_price'] = web3.eth.gas_price

    l1_config = app.config["networks"][l1_network]
    ethweb3 = Web3(Web3.HTTPProvider(l1_config["l1_url"]))
    # Check if connected successfully
    if not ethweb3.is_connected():
        print("Failed to connect to L1 node.")
        raise

    l2_proxy_info = get_diamond_proxy_info(ethweb3, basic_info["state_transition"])
    return {
        'name': chain_name,
        'chain_id': config["chain_id"],
        'basic_info': basic_info,
        'l2': l2_proxy_info,
    }

def get_shared_bridge_details(l1_network, l2_network):
    l1_config = app.config["networks"][l1_network]
    l2_config = l1_config["shared_bridges"][l2_network]
    chains = [
        get_shared_bridge_chain_id_details(l1_network, l2_config, name) for name in l2_config["chains"].keys()
    ]

    return {
        'l1_network': l1_network,
        'shared_bridge_name': l2_network,
        'chain_count': len(l2_config["chains"]),
        'chains': chains,
        'explorer_prefix': l1_config['explorer_prefix'],
        'explorer_address_prefix': l1_config['explorer_prefix'] + "address/",
    }




def get_diamond_proxy_info(provider, address):
    l2 = {}
    contract_abi = [
        {
            "name": "getPriorityQueueSize",
            "inputs": [],
            "outputs": [
                {
                    "type": "uint256"
                }
            ],
            "type": "function"
        },
        {
            "name": "getTotalBatchesCommitted",
            "inputs": [],
            "outputs": [
                {
                    "type": "uint256"
                }
            ],
            "type": "function"
        },
        {
            "name": "getTotalBatchesVerified",
            "inputs": [],
            "outputs": [
                {
                    "type": "uint256"
                }
            ],
            "type": "function"
        },
        {
            "name": "getTotalBatchesExecuted",
            "inputs": [],
            "outputs": [
                {
                    "type": "uint256"
                }
            ],
            "type": "function"
        },
        

        {
            "name": "getL2BootloaderBytecodeHash",
            "inputs": [],
            "outputs": [
                {
                    "type": "bytes32"
                }
            ],
            "type": "function"
        },
        {
            "name": "getL2DefaultAccountBytecodeHash",
            "inputs": [],
            "outputs": [
                {
                    "type": "bytes32"
                }
            ],
            "type": "function"
        },
        
        {
            "name": "getProtocolVersion",
            "inputs": [],
            "outputs": [
                {
                    "type": "uint256"
                }
            ],
            "type": "function"
        },
        


    ]

    contract = provider.eth.contract(address=Web3.to_checksum_address(address), abi=contract_abi)

    l2['l1_priority_queue_size'] = contract.functions.getPriorityQueueSize().call()

    l2['l1_batches'] = {
        'committed': contract.functions.getTotalBatchesCommitted().call(),
        'verified': contract.functions.getTotalBatchesVerified().call(),
        'executed': contract.functions.getTotalBatchesExecuted().call()
    }

    l2['bootloader'] = contract.functions.getL2BootloaderBytecodeHash().call().hex()
    l2['accountcode'] = contract.functions.getL2DefaultAccountBytecodeHash().call().hex()
    l2['protocol_version'] = contract.functions.getProtocolVersion().call()
    return l2


# Get information about single bridge (based on l1 and l2 network names)
def get_single_bridge_details(l1_network, l2_network):
    l1_config = app.config["networks"][l1_network]
    l2_config = l1_config["single_bridges"][l2_network]
    
    l2 = {
        'url': l2_config["l2_url"],
        'name': l2_network,
    }   

    l1 = {
        'url': l1_config["l1_url"],
        'name': l1_network,
    }
    web3 = Web3(Web3.HTTPProvider(l2_config["l2_url"]))
    # Check if connected successfully
    if not web3.is_connected():
        print("Failed to connect to L2 node.")
        raise
    
    ethweb3 = Web3(Web3.HTTPProvider(l1_config["l1_url"]))
    # Check if connected successfully
    if not ethweb3.is_connected():
        print("Failed to connect to L1 node.")
        raise

    
    l2['chain_id'] = web3.eth.chain_id
    l1['chain_id'] = ethweb3.eth.chain_id

    l2['proxy_contract'] =  get_main_contract(l2_config["l2_url"])
    

    l2['l1_balance'] = ethweb3.eth.get_balance(Web3.to_checksum_address(l2['proxy_contract']))

    l2['l1_balance_in_ether'] = Web3.from_wei(l2['l1_balance'], 'ether')

    l2_ether_contract_abi = [
        {
            "name": "totalSupply",
            "inputs": [],
            "outputs": [
                {
                    "type": "uint256"
                }
            ],
            "type": "function"
        },
    ]

    l2_ether_contract = web3.eth.contract(address=Web3.to_checksum_address("0x000000000000000000000000000000000000800a"), abi=l2_ether_contract_abi)

    l2_context_contract_abi = [
        {
            "name": "gasPrice",
            "inputs": [],
            "outputs": [
                {
                    "type": "uint256"
                }
            ],
            "type": "function"
        },
    ]

    l2_context_contract = web3.eth.contract(address=Web3.to_checksum_address("0x000000000000000000000000000000000000800b"), abi=l2_context_contract_abi)


    l2['gas_price'] = l2_context_contract.functions.gasPrice().call()
    l2['gas_price_gwei'] = Web3.from_wei(l2['gas_price'], 'gwei')

    l2['balance'] = l2_ether_contract.functions.totalSupply().call()
    l2['balance_in_ether'] = Web3.from_wei(l2['balance'], 'ether')

    l2.update(get_diamond_proxy_info(ethweb3, l2['proxy_contract']))
    

    l1_state_storage = get_l1_state_storage(l1_config["l1_url"], l2['proxy_contract'], "latest")

    l2["l1_state"] = l1_state_storage
    return (l1, l2)

if __name__ == '__main__':
    app.run(debug=True)
