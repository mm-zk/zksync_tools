# Prove & Execute
A small tool that allows you to call prove & execute on Zksync diamond proxy chains.


## TL;DR

To fake-proof & execute all the committed batches, simply run:

```
cargo run -- --address $DIAMOND_PROXY_ADDR --private-key $PRIVATE_KEY fake-prove-and-execute
```


## E2E example.

Here you can see the process in more details.

Private key is from the 'rich' account in anvil.

For production - you have to specify the key that has permissions to Prove & Execute.

```shell
DIAMOND_PROXY_ADDR=0x...
PRIVATE_KEY=0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
cargo run -- --address $DIAMOND_PROXY_ADDR show 

// Total batches committed: 3
// Total batches verified: 2 
// Total batches executed: 2
```

Let's prove & execute batch 3, first compute the public input:

```shell
cargo run -- --address $DIAMOND_PROXY_ADDR public-input --start 3 --end 3

// Snark public input for range 3-3: 0x00000000419fcb7fd9a0896cff3ddfbcdd347697bc59ecadff183036f287b74d
```

Then fake-prove (copy the public input from above):

```shell
cargo run -- --address $DIAMOND_PROXY_ADDR --private-key $PRIVATE_KEY fake-prove --start 3 --end 3 --public-input 00000000419fcb7fd9a0896cff3ddfbcdd347697bc59ecadff183036f287b74d
```

And then to 'execute':

```shell
cargo run -- --address $DIAMOND_PROXY_ADDR  --private-key $PRIVATE_KEY execute --start 3 --end 3
```

And voila:

```shell
cargo run -- --address $DIAMOND_PROXY_ADDR show 

// Total batches committed: 3
// Total batches verified: 3 
// Total batches executed: 3
```




## Show

Will show you basic info about a given diamond proxy:

```shell
cargo run -- --address 0x51f119DCBBAD1B737C6a3c63063ad44B7E315399 show
```


```
Using diamond Proxy: 0x51f119DCBBAD1B737C6a3c63063ad44B7E315399
Using Verifier: 0x0906761B78eF7dD823cbC046C8F5d3F3eA8102E2
Total batches committed: 2
Total batches verified: 2
Total batches executed: 2
Batches recovered from sequencer: 2
Protocol version: 0.28.0
```

## Public inputs

Will compute the public input values (both FRI and SNARK) for given range of blocks:

```shell
cargo run -- --address 0x51f119DCBBAD1B737C6a3c63063ad44B7E315399 public-input --start 1 --end 2
```

```
FRI Public input for batch 1: 0xf37ff1e07e2922bdc9b85300f6bf94907826242163fbd4dcf0bdb881b58f3b5e
SNARK Public input for batch 1: 0x00000000f37ff1e07e2922bdc9b85300f6bf94907826242163fbd4dcf0bdb881
FRI Public input for batch 2: 0xb77d6bccb8e7328b61fd8fd427125d4b01311feb7edb8c1ae4a7390b733affc8
SNARK Public input for batch 2: 0x00000000b77d6bccb8e7328b61fd8fd427125d4b01311feb7edb8c1ae4a7390b
Snark public input for range 1-2: 0x000000004bdb65a385be0d18b7a75d686d4a41866a4a81a4324bab55fabc82b3
```


## Prove

Will use a given SNARK as a proof for a range of blocks.
```
cargo run -- --address 0xE43497368Cd61e38F7976ab065DEb61fEE374248 prove --start 2 --end 4 --snark-path merged_3.snark
```

if your snark starts earlier, you can specify `--snark_start` argument.

If you specify `--private-key` - the tool will create a transaction - otherwise it will just do a 'call'.

## Fake Prove

If you don't want to spend time creating a snark, you can try to use the 'mock' verifier -- then you have to compute the proper public input, and pass it directly:


```
cargo run -- --address 0x19ed66b7e720aA741a748d1779da66D30FDCa549  fake-prove --start 1 --end 1  --public-input fbc56cf7a8cf1666e1d6a1fb80f23f9efa07965eed1016ab
```

If you specify `--private-key` - the tool will create a transaction - otherwise it will just do a 'call'.


## Execute

To execute a range of batches - run: 

```
cargo run -- --address 0x51f119DCBBAD1B737C6a3c63063ad44B7E315399  execute --start 2 --end 2
```

If you specify `--private-key` - the tool will create a transaction - otherwise it will just do a 'call'.
