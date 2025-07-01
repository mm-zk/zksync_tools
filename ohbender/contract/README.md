
To create using script:

forge script script/Counter.s.sol:CounterScript --rpc-url http://localhost:8545 --private-key 0x27593fea79697e947890ecbecce7901b0008345e5d7259710d0dd5e500d040be --broadcast --legacy




proofPublicInput[i] = uint256(
    keccak256(abi.encodePacked(prevBatchStateCommitment, currentBatchStateCommitment, currentBatchCommitment))
) >> PUBLIC_INPUT_SHIFT;


