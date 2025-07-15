// SPDX-License-Identifier: MIT
pragma solidity ^0.8.21;

interface IEntryPoint {}

struct PackedUserOperation {
    address sender;
    uint256 nonce;
    bytes initCode;
    bytes callData;
    bytes32 accountGasLimits;
    uint256 preVerificationGas;
    bytes32 gasFees;
    bytes paymasterAndData;
    bytes signature;
}

interface IAccount {
    function validateUserOp(
        PackedUserOperation calldata userOp,
        bytes32 userOpHash,
        uint256 missingAccountFunds
    ) external returns (uint256 validationData);
}

contract DummyAccount is IAccount {
    IEntryPoint public immutable entryPoint;

    address public owner;

    constructor(IEntryPoint _entryPoint) {
        entryPoint = _entryPoint;
        owner = msg.sender;
    }

    receive() external payable {}

    function validateUserOp(
        PackedUserOperation calldata userOp,
        bytes32,
        uint256
    ) external override returns (uint256 validationData) {
        require(msg.sender == address(entryPoint), "not from entry point");

        // Accept only if signature is literally `0x01`
        if (userOp.signature.length != 1 || userOp.signature[0] != 0x01) {
            return 1; // non-zero => invalid signature
        }

        return 0; // valid
    }

    function execute(
        address dest,
        uint256 value,
        bytes calldata func
    ) external {
        require(msg.sender == address(entryPoint), "only entry point");

        (bool success, ) = dest.call{value: value}(func);
        require(success, "call failed");
    }
}
