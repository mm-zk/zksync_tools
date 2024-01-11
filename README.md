# zksync_tools

## tx_prover
Goal - prove that a given transaction was executed in L2.

Steps:
* check that transaction belongs to a given block - by checking if it hashes (together with other transactions) into the block hash
* check that block hash belongs to the chain: (TODO) - by asking for next blocks, and checking that parent block is set
* verify that the final block hash in the batch is correct - by checking the contents of the calldata.

### Problems
The state diff in calldata contains repeated writes (so index -> value). Need a trustless way to find out what is the index of the SystemContext entry that holds the lastest block hash.

