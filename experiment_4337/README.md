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


