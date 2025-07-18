# Simple interop (v29)


## Tool

Tool allows you to quickly see all the outgoing interop messages, and prove that they were received on destination chain.

First, send the interop message (like in the instructions below):

```
cast send -r http://localhost:3050 0x0000000000000000000000000000000000008008  "sendToL1(bytes)" "0x1234" --private-key $PRIVATE_KEY
TX_ID=0xc9f09ef3cf6c31fe9f1376845a2900da7b68e6f31db1c10c90d965ec8a8a76d2
```

Collect the transaction id - and then call:

```
cargo run show-interop-message --source-rpc http://localhost:3050 --source-tx $TX_ID
```

This will print you the list of interop transactions.


You can also prove the interop message on the destination chain using:

```
cargo run prove-interop-message --source-rpc http://localhost:3050 --source-tx $TX_ID --target-rpc http://localhost:3050
```


## Manual operations


First - follow instructions from https://github.com/matter-labs/zksync-era/blob/main/docs/src/guides/launch.md to setup era chain & gateway.


Important addresses:

```
export const L2_INTEROP_ROOT_STORAGE_ADDRESS = '0x0000000000000000000000000000000000010008';
export const L2_MESSAGE_VERIFICATION_ADDRESS = '0x0000000000000000000000000000000000010009';

```




Give funds to Era:

```
zkstack dev rich-account
ACCOUNT="0x36615Cf349d7F6344891B1e7CA7C72883F5dc049"
PRIVATE_KEY="0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110"
```

Send message on L1 messenger.


```
cast send -r http://localhost:3050 0x0000000000000000000000000000000000008008  "sendToL1(bytes)" "0x1234" --private-key $PRIVATE_KEY
```
Remember transaction hash and block number - in my case 

```
TX_HASH="0xc9f09ef3cf6c31fe9f1376845a2900da7b68e6f31db1c10c90d965ec8a8a76d2"
BLOCK_NUMBER=0x2b
```



To get the message hash:

```
cast call -r http://localhost:3050 0x0000000000000000000000000000000000008008  "sendToL1(bytes)" "0x1234"                           
> 0x56570de287d73cd1cb6092bb8fdee6173974955fdef345ae579ee9f475ea7432
```



Fetch proof from the sequencer..


```
curl --request POST \
  --url http://localhost:3050/ \
  --header 'Content-Type: application/json' \
  --data '{
      "jsonrpc": "2.0",
      "id": 1,
      "method": "zks_getL2ToL1LogProof",
      "params": [
        0xc9f09ef3cf6c31fe9f1376845a2900da7b68e6f31db1c10c90d965ec8a8a76d2
      ]
    }'
```

```json
{"jsonrpc":"2.0","id":1,"result":{"proof":["0x010f050000000000000000000000000000000000000000000000000000000000","0x72abee45b59e344af8a6e520241c4744aff26ed411f4c4b00f8af09adada43ba","0xc3d03eebfd83049991ea3d3e358b6712e7aa2e2e63dc2d4b438987cec28ac8d0","0xe3697c7f33c31a9b0f0aeb8542287d0d21e8c4cf82163d0c44c7a98aa11aa111","0x199cc5812543ddceeddd0fc82807646a4899444240db2c0d2f20c3cceb5f51fa","0xe4733f281f18ba3ea8775dd62d2fcd84011c8c938f16ea5790fd29a03bf8db89","0x1798a1fd9c8fbb818c98cff190daa7cc10b6e5ac9716b4a2649f7c2ebcef2272","0x66d7c5983afe44cf15ea8cf565b34c6c31ff0cb4dd744524f7842b942d08770d","0xb04e5ee349086985f74b73971ce9dfe76bbed95c84906c5dffd96504e1e5396c","0xac506ecb5465659b3a927143f6d724f91d8d9c4bdb2463aee111d9aa869874db","0x124b05ec272cecd7538fdafe53b6628d31188ffb6f345139aac3c3c1fd2e470f","0xc3be9cbd19304d84cca3d045e06b8db3acd68c304fc9cd4cbffe6d18036cb13f","0xfef7bd9f889811e59e4076a0174087135f080177302763019adaf531257e3a87","0xa707d1c62d8be699d34cb74804fdd7b4c568b6c1a821066f126c680d4b83e00b","0xf6e093070e0389d2e529d60fadb855fdded54976ec50ac709e3a36ceaa64c291","0xe4ed1ec13a28c40715db6399f6f99ce04e5f19d60ad3ff6831f098cb6cf75944","0x0000000000000000000000000000000000000000000000000000000000000015","0x156467afe10e8eb1dcac04cca213c53d4bade9d73edfe9410ea45c86ebd9804e","0xcc4c41edb0c2031348b292b768e9bac1ee8c92c09ef8a3277c2ece409c12d86a","0x183a40fea23b03351928919261dfc02e45b94b564e263d339a601b9740112ccc","0x4cd95f8962e2e3b5f525a0f4fdfbbf0667990c7159528a008057f3592bcb2c06","0x112038ecbdf21c5fc2ef97d9ec047402bba05d6ff2aa1b304dcfac974ebaa109","0x0000000000000000000000000000001900000000000000000000000000000003","0x00000000000000000000000000000000000000000000000000000000000001fa","0x0102000100000000000000000000000000000000000000000000000000000000","0xf84927dc03d95cc652990ba75874891ccc5a4d79a0e10a2ffdd238a34a39f828","0x2c21d37d09509f7ccb3d7d93c86ff4d7b29246113b0032dec9688c9e119d6872"],"id":0,"root":"0xeb6861fa2f2fd60c93dba1343bf85e2312811fd23549b3df2b8017102430f086"}}
```


But with gateway - diffrent proof:

```
curl --request POST \
  --url http://localhost:3050/ \
  --header 'Content-Type: application/json' \
  --data '{
      "jsonrpc": "2.0",
      "id": 1,
      "method": "zks_getL2ToL1LogProof",
      "params": [
        "0xc9f09ef3cf6c31fe9f1376845a2900da7b68e6f31db1c10c90d965ec8a8a76d2", 0, "proof_based_gw"   
      ]
    }'
```

["0x010f050000000000000000000000000000000000000000000000000000000000","0x72abee45b59e344af8a6e520241c4744aff26ed411f4c4b00f8af09adada43ba","0xc3d03eebfd83049991ea3d3e358b6712e7aa2e2e63dc2d4b438987cec28ac8d0","0xe3697c7f33c31a9b0f0aeb8542287d0d21e8c4cf82163d0c44c7a98aa11aa111","0x199cc5812543ddceeddd0fc82807646a4899444240db2c0d2f20c3cceb5f51fa","0xe4733f281f18ba3ea8775dd62d2fcd84011c8c938f16ea5790fd29a03bf8db89","0x1798a1fd9c8fbb818c98cff190daa7cc10b6e5ac9716b4a2649f7c2ebcef2272","0x66d7c5983afe44cf15ea8cf565b34c6c31ff0cb4dd744524f7842b942d08770d","0xb04e5ee349086985f74b73971ce9dfe76bbed95c84906c5dffd96504e1e5396c","0xac506ecb5465659b3a927143f6d724f91d8d9c4bdb2463aee111d9aa869874db","0x124b05ec272cecd7538fdafe53b6628d31188ffb6f345139aac3c3c1fd2e470f","0xc3be9cbd19304d84cca3d045e06b8db3acd68c304fc9cd4cbffe6d18036cb13f","0xfef7bd9f889811e59e4076a0174087135f080177302763019adaf531257e3a87","0xa707d1c62d8be699d34cb74804fdd7b4c568b6c1a821066f126c680d4b83e00b","0xf6e093070e0389d2e529d60fadb855fdded54976ec50ac709e3a36ceaa64c291","0xe4ed1ec13a28c40715db6399f6f99ce04e5f19d60ad3ff6831f098cb6cf75944","0x0000000000000000000000000000000000000000000000000000000000000015","0x156467afe10e8eb1dcac04cca213c53d4bade9d73edfe9410ea45c86ebd9804e","0xcc4c41edb0c2031348b292b768e9bac1ee8c92c09ef8a3277c2ece409c12d86a","0x183a40fea23b03351928919261dfc02e45b94b564e263d339a601b9740112ccc","0x4cd95f8962e2e3b5f525a0f4fdfbbf0667990c7159528a008057f3592bcb2c06","0x112038ecbdf21c5fc2ef97d9ec047402bba05d6ff2aa1b304dcfac974ebaa109","0x0000000000000000000000000000003100000000000000000000000000000001","0x00000000000000000000000000000000000000000000000000000000000001fa","0x0101000100000000000000000000000000000000000000000000000000000000","0xf84927dc03d95cc652990ba75874891ccc5a4d79a0e10a2ffdd238a34a39f828"]



Now this block has travelled to settlement layer (Gateway).


```
cast receipt -r http://localhost:3050 0xc9f09ef3cf6c31fe9f1376845a2900da7b68e6f31db1c10c90d965ec8a8a76d2
```
And you get:
```
blockNumber             43
...
l1BatchNumber             "0x16"
l1BatchTxIndex             "0x0"
```


(alternatively you can query getBlockDetails:)

```
curl --request POST \
  --url localhost:3050 \
  --header 'Content-Type: application/json' \
  --data '{
      "jsonrpc": "2.0",
      "id": 1,
      "method": "zks_getBlockDetails",
      "params": [43]
    }'
```

And now we see, that it was a part of batch 22:
```
"l1BatchNumber":22
```

```
curl --request POST \
  --url localhost:3050 \
  --header 'Content-Type: application/json' \
  --data '{
      "jsonrpc": "2.0",
      "id": 1,
      "method": "zks_getL1BatchDetails",
      "params": [22]
    }'
```

```
"executeTxHash":"0xe00fc1970fbc17d90873dcc7c4a07400d7658cbe7b6be7f98052fe2f0c6b35ab
```

And now we query **gateway**

```
cast receipt -r http://localhost:3150 0xe00fc1970fbc17d90873dcc7c4a07400d7658cbe7b6be7f98052fe2f0c6b35ab
```

```
blockNumber             49
l1BatchNumber             "0x19"
l1BatchTxIndex             "0x2"
```


So now let's see if Gateway's interop was already updated in our Era chain.
Gateway's chain is is 506:

```
cast chain-id -r http://localhost:3150
> 506
```





trying to get the interop roots, but I keep getting 0:

```
 cast call -r http://localhost:3050 0x0000000000000000000000000000000000010008 "interopRoots(uint256,uint256)" 506 49
```

If this is still 0 - let's push some transactions to Era, to make it generate new batches and update interop:

```
cast send -r http://localhost:3050 0x89551e10fAe767C6EE75e90E9E04F48fB19e6A15 --value 100  --private-key $PRIVATE_KEY --gas-limit 10000000
```

And now we have the interop root from Gateway:

```
0x0df3f90e2c0d78bda1407b6b96864a1a5a13df7549867e897dd64d6b9aab01e0
```




And now we call L2 message verification contract:


Use batch id from 'era'  + index in batch.

Tx in block is also 0 (TODO: how to get it?)

First attempt:

And it didn't work (no surprise)

```
cast call 0x0000000000000000000000000000000000010009 \
  "proveL2MessageInclusionShared(uint256,uint256,uint256,(uint16,address,bytes),bytes32[])" \
  271 \
  22 \
  0 \
  "(0,0x36615Cf349d7F6344891B1e7CA7C72883F5dc049,0x1234)" \
  "["0x010f050000000000000000000000000000000000000000000000000000000000","0x72abee45b59e344af8a6e520241c4744aff26ed411f4c4b00f8af09adada43ba","0xc3d03eebfd83049991ea3d3e358b6712e7aa2e2e63dc2d4b438987cec28ac8d0","0xe3697c7f33c31a9b0f0aeb8542287d0d21e8c4cf82163d0c44c7a98aa11aa111","0x199cc5812543ddceeddd0fc82807646a4899444240db2c0d2f20c3cceb5f51fa","0xe4733f281f18ba3ea8775dd62d2fcd84011c8c938f16ea5790fd29a03bf8db89","0x1798a1fd9c8fbb818c98cff190daa7cc10b6e5ac9716b4a2649f7c2ebcef2272","0x66d7c5983afe44cf15ea8cf565b34c6c31ff0cb4dd744524f7842b942d08770d","0xb04e5ee349086985f74b73971ce9dfe76bbed95c84906c5dffd96504e1e5396c","0xac506ecb5465659b3a927143f6d724f91d8d9c4bdb2463aee111d9aa869874db","0x124b05ec272cecd7538fdafe53b6628d31188ffb6f345139aac3c3c1fd2e470f","0xc3be9cbd19304d84cca3d045e06b8db3acd68c304fc9cd4cbffe6d18036cb13f","0xfef7bd9f889811e59e4076a0174087135f080177302763019adaf531257e3a87","0xa707d1c62d8be699d34cb74804fdd7b4c568b6c1a821066f126c680d4b83e00b","0xf6e093070e0389d2e529d60fadb855fdded54976ec50ac709e3a36ceaa64c291","0xe4ed1ec13a28c40715db6399f6f99ce04e5f19d60ad3ff6831f098cb6cf75944","0x0000000000000000000000000000000000000000000000000000000000000015","0x156467afe10e8eb1dcac04cca213c53d4bade9d73edfe9410ea45c86ebd9804e","0xcc4c41edb0c2031348b292b768e9bac1ee8c92c09ef8a3277c2ece409c12d86a","0x183a40fea23b03351928919261dfc02e45b94b564e263d339a601b9740112ccc","0x4cd95f8962e2e3b5f525a0f4fdfbbf0667990c7159528a008057f3592bcb2c06","0x112038ecbdf21c5fc2ef97d9ec047402bba05d6ff2aa1b304dcfac974ebaa109","0x0000000000000000000000000000003100000000000000000000000000000001","0x00000000000000000000000000000000000000000000000000000000000001fa","0x0101000100000000000000000000000000000000000000000000000000000000","0xf84927dc03d95cc652990ba75874891ccc5a4d79a0e10a2ffdd238a34a39f828"]" \
  --rpc-url http://localhost:3050
```