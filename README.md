# zksync_tools

## tx_prover
Goal - prove that a given transaction was executed in L2.

Usage:
```
python3 tx_prover/main.py 0xdc77b749534a2114a2bd10afd9c42c34205b0c8381b4eb59acfdcfc5576eb171
```

requires `web3` and `eth-abi` python packages.

Internal details:
* check that transaction belongs to a given block - by checking if it hashes (together with other transactions) into the block hash
* check if the block hash is in the correct place in the merkle tree (last 257 block hashes are always persisted)
* check if the batch is committed and proven


## Current status
Adding support for shared bridges.

Support for blobs is not there yet.

You can build a docker image via:

```shell
docker build -t hyperexplorer .
```