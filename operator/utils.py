import requests
import json

def get_main_contract(url):
    headers = {"Content-Type": "application/json"}
    data = {"jsonrpc": "2.0", "id": 1, "method": "zks_getMainContract", "params": []}
    response = requests.post(url, headers=headers, data=json.dumps(data))
    return response.json()["result"]


def get_batch_details(url, batch_number):
    headers = {"Content-Type": "application/json"}
    data = {"jsonrpc": "2.0", "id": 1, "method": "zks_getL1BatchDetails", "params": [batch_number]}
    response = requests.post(url, headers=headers, data=json.dumps(data))
    return response.json()["result"]