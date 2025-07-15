Experiments with 4337.

Setup:

```shell
# start local anvil
anvil

git clone https://github.com/eth-infinitism/account-abstraction
cd account-abstraction
yarn install
yarn compile

yarn hardhat deploy --network dev


# This should return the code for this 4337 user contract now.
cast code -r http://localhost:8545 0x4337084D9E255Ff0702461CF8895CE9E3b5Ff108
```


Now let's build the 'dummy' smart contract (that will accept anything with '1' as signature).

Private key is from Anvil:

```
forge create src/DummyAccount.sol:DummyAccount --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 --constructor-args 0x4337084D9E255Ff0702461CF8895CE9E3b5Ff108

# Deployed to: 0xDc64a140Aa3E981100a9becA4E685f962f0cF6C9
DUMMY=0xDc64a140Aa3E981100a9becA4E685f962f0cF6C9
```

Fund the account
```
cast send $DUMMY --value 1ether --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
cast balance $DUMMY
```

Fund the entrypoint:
```
cast send $ENTRYPOINT "depositTo(address)" $DUMMY --value 1ether --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
cast call $ENTRYPOINT "getDepositInfo(address)(uint256,bool,uint112,uint32,uint48)" $DUMMY
```



Now let's do the transfer from dummy to some new account:

```
NEW_ACC=0x44861657490dF37CF1f9b2e1CA8404F9684BC852
cast balance $NEW_ACC 
# should be 0
```

Then do the actual 4337:

```
ENTRYPOINT=0x4337084D9E255Ff0702461CF8895CE9E3b5Ff108
CALLDATA=`cast calldata "execute(address,uint256,bytes)" $NEW_ACC 1000000 0x`

GAS_FEES=0x000000000000000000000000040be400000000000000000000000000040be400

ACCOUNT_GAS_LIMITS=0x0000000000000000000000001f000100000000000000000000000000001f0000


cast send $ENTRYPOINT \
  "handleOps((address,uint256,bytes,bytes,bytes32,uint256,bytes32,bytes,bytes)[],address)" \
"[($DUMMY,0x0,0x,$CALLDATA,$ACCOUNT_GAS_LIMITS,0x1f0000,$GAS_FEES,0x,0x01)]" \
  $ENTRYPOINT \
  --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
```

And finally check the balance:

```
cast balance $NEW_ACC 
# should be > 0
```


## Now on zkos 

in account abstraction repo in hardhat.config.ts:
add to HardhatUserConfig as a new network
```
    zksyncos: { 
      url: 'http://localhost:3050',
      accounts: ['0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110']

    },
```
then run:
```
yarn hardhat deploy --network zksyncos
```



```
forge create -r http://localhost:3050 src/DummyAccount.sol:DummyAccount --private-key 0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110 --constructor-args 0x4337084D9E255Ff0702461CF8895CE9E3b5Ff108

# Deployed to: 0xf1Ebfaa992854ECcB01Ac1F60e5b5279095cca7F
DUMMY=0xf1Ebfaa992854ECcB01Ac1F60e5b5279095cca7F
```

```
cast send -r http://localhost:3050 $DUMMY --value 1ether --private-key 0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110
cast balance -r http://localhost:3050 $DUMMY
cast send -r http://localhost:3050 $ENTRYPOINT "depositTo(address)" $DUMMY --value 1ether --private-key 0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110
cast call -r http://localhost:3050 $ENTRYPOINT "getDepositInfo(address)(uint256,bool,uint112,uint32,uint48)" $DUMMY
```


```
ENTRYPOINT=0x4337084D9E255Ff0702461CF8895CE9E3b5Ff108
CALLDATA=`cast calldata "execute(address,uint256,bytes)" $NEW_ACC 1000000 0x`

GAS_FEES=0x000000000000000000000000040be400000000000000000000000000040be400

ACCOUNT_GAS_LIMITS=0x0000000000000000000000001f000100000000000000000000000000001f0000


cast send -r http://localhost:3050 $ENTRYPOINT \
  "handleOps((address,uint256,bytes,bytes,bytes32,uint256,bytes32,bytes,bytes)[],address)" \
"[($DUMMY,0x0,0x,$CALLDATA,$ACCOUNT_GAS_LIMITS,0x1f0000,$GAS_FEES,0x,0x01)]" \
  $ENTRYPOINT \
  --private-key 0x7726827caac94a7f9e1b160f7ea819f172f7b6f9d2a97f992c38edeab82d4110

```


```
cast balance -r http://localhost:3050 $NEW_ACC 
# should be > 0
```
