# Governance tool

Tool to manage and execute governance upgrades for local instances.

It allows you to easily add and replace facets on your Diamond Proxy (allowing a lot faster iteration and debugging - as you can add custom implementations of methods without having to restart your whole pipeline.)


Here's the example on how to add a new facet ('setVerifier'). 

First: deploy a new contract with such facet.

Then you can run:

```shell
cargo run -- --address 0x19ed66b7e720aA741a748d1779da66D30FDCa549 --method-name "setVerifier(address)" --new-address 0xc6e7DF5E7b4f2A278906862b61205850344D4e7d --governance-private-key 0xe8a938f64456a34b68f47f0424ea731c1e213cbb65b27777091312f47edf00bf --chain-id 270
```

Where:
* address is the address of the diamond proxy
* new address is where the new method should point at
* governance private key - look it up in your ecosystem wallet settings - this will be governors key.



TODO:
* pass bridgehub as main address
* detect add / replace for facets


