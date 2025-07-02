OhBender setup.

Based off zksync-era commit: 9ca2a85f70a90223efa81d7ac4510cbeb87c1b91

(also used docker for proving)
I uses airbender 0.2.0 (and same for zkos wrapper)
SNARK verifier hash is:

0x3f0828d2239746bb61a9cee78016d3e191850c941f91eefd8af849716d06d3ea


# CLI tool to handle FRI merging


Can parse FRI and SNARK files (both ProgramProof json or JSON fetched from sequencer API)
```shell
cargo run parse-fri 1.fri
```

Can merge FRIs into a single FRI (using universal verifier).

```shell
cargo run --release merge-fri 1.fri 2.fri 3.fri 4.fri --output foo.json --tmp-dir tmp_results
```


# Running ohbender

* checkout zksync-era (zksync-os-integration branch)  - suggested commit 193db617bc83d283d197200cf0693389be45335d

* Run the sequencer (zkstack ecosystem init + server) - more details in the zksync era README

* Run the FRI prover from zksync-era

And now - instead of running the SNARK wrapper from zksync era (which would SNARK wrap every block):

```shell
cargo run --release --features gpu -- run --binary ../../zksync-era/execution_environment/app.bin --output /tmp/runner --l1-rpc http://localhost:8545 --sequencer-rpc http://localhost:3053 --sequencer-prover-api http://localhost:3124
```

This will talk to sequencer, get bridgehub info, talk to L1 - figure out all the blocks that are not proven, fetch them from sequencer, FRI-merge them,
then SNARK wrap the final thing - and send it (currently only 'call') to L1.
If the whole run is successful, it means that the final 'call' was with proper proof.

Things to add:
* run in a loop
* send a transaction (instead of just a 'call')
* also call 'execute blocks'
* iterate over a list of chains
* get FRI from some 'common' place instead of having to talk to sequencer.