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
