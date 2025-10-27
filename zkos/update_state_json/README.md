# update state.json

Updates state.json file in zksync-os-server based off the given era-contracts.
 

Example use case:

To generate state & genesis locally:

```
 ERA_CONTRACTS_TAG=zkos-v0.29.7  ./update.sh
```

This will use zkstack cli from 'main', and will do updates to 'main' zksync-os-server.
You can override those by setting ZKSYNC_ERA_STACK_CLI_TAG and ZKSYNC_OS_SERVER_TAG respectively.



## Testing

then (assuming that genesis didn't change), you can test if by running

```shell
anvil --load-state zkos-l1-state.json
```

And on separate screen:

```shell
rm -rf repos/zksync-os-server/db
rm -rf ecosystem/local_v1/chains/era1/db

cd repos/zksync-os-server
general_zkstack_cli_config_dir=../../ecosystem/local_v1/chains/era1 cargo run
```

## Preparing PR

You can also ask the tool to prepare the zksync-os-server PR for you:

```
COMMIT_CHANGES=true ERA_CONTRACTS_TAG=zkos-v0.29.7  ./update.sh
```
