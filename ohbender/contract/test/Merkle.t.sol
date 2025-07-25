pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";

contract MerkleTest is Test {
    function setUp() public {}

    function efficientHash(
        bytes32 _lhs,
        bytes32 _rhs
    ) internal pure returns (bytes32 result) {
        assembly {
            mstore(0x00, _lhs)
            mstore(0x20, _rhs)
            result := keccak256(0x00, 0x40)
        }
    }

    function calculateRootPaths(
        bytes32[] memory _startPath,
        bytes32[] memory _endPath,
        uint256 _startIndex,
        bytes32[] memory _itemHashes
    ) internal pure returns (bytes32) {
        uint256 pathLength = _startPath.length;
        if (pathLength != _endPath.length) {
            revert("fail");
        }
        if (pathLength >= 256) {
            revert("fail");
        }
        uint256 levelLen = _itemHashes.length;
        // Edge case: we want to be able to prove an element in a single-node tree.
        if (pathLength == 0 && (_startIndex != 0 || levelLen != 1)) {
            revert("fail");
        }
        if (levelLen == 0) {
            revert("fail");
        }
        if (_startIndex + levelLen > (1 << pathLength)) {
            revert("fail");
        }
        bytes32[] memory itemHashes = _itemHashes;

        for (uint256 level; level < pathLength; level = level + 1) {
            uint256 parity = _startIndex % 2;
            // We get an extra element on the next level if on the current level elements either
            // start on an odd index (`parity == 1`) or end on an even index (`levelLen % 2 == 1`)
            uint256 nextLevelLen = levelLen / 2 + (parity | (levelLen % 2));
            for (uint256 i; i < nextLevelLen; i = i + 1) {
                bytes32 lhs = (i == 0 && parity == 1)
                    ? _startPath[level]
                    : itemHashes[2 * i - parity];
                bytes32 rhs = (i == nextLevelLen - 1 &&
                    (levelLen - parity) % 2 == 1)
                    ? _endPath[level]
                    : itemHashes[2 * i + 1 - parity];
                itemHashes[i] = efficientHash(lhs, rhs);
            }
            levelLen = nextLevelLen;
            _startIndex /= 2;
        }

        return itemHashes[0];
    }

    function test_Increment() public {
        console.log("Hello");
        bytes32[] memory startPath = new bytes32[](3);
        // ok
        startPath[
            0
        ] = 0x86483764f0d42e8fc592e40d94550636e1a902fe1299e11d1525aae5377b8ade;
        // ok
        startPath[
            1
        ] = 0x31b4ea310e6748649c15d3e1a98ed1be9d3f70cc03c321f27bf432b4cde139b8;
        startPath[
            2
        ] = 0x6d28f81de01a76f8d3bc2782863418e914bd7ecdd31286ca9f93214b06d3d2fd;

        bytes32[] memory endPath = new bytes32[](3);

        endPath[
            0
        ] = 0x3492cc0bf469717d0a6b72e9fe80c17d274042c80d7180b78d61c80ab078ac9e;
        endPath[
            1
        ] = 0xa37b182add1809f12320c79d832c7dd59e4639fc0fbf66fe8cc04a771dcdcaec;
        endPath[
            2
        ] = 0xef3b039351942ed94ed2f1082f699a104a2bf101cd7fa95c62929f17709923fe;

        uint256 startIndex = 0;

        bytes32[] memory itemHashes = new bytes32[](7);
        itemHashes[
            0
        ] = 0x3db875b71f4a77558dcc419db20e6038d4718290786f5c88a793f75a88552dde;
        itemHashes[
            1
        ] = 0x86483764f0d42e8fc592e40d94550636e1a902fe1299e11d1525aae5377b8ade;
        itemHashes[
            2
        ] = 0x1f952e46ebb444dd9f680711c0f55427f00bda0f491d7bdc394a4672bb9bac4b;
        itemHashes[
            3
        ] = 0x119c64eb5cbd0ff9c6b1e279412fac3e8941a8bd9551bb04e6bb6ef39716d7eb;
        itemHashes[
            4
        ] = 0x5646b058be2275d7b18ca90fb56412bf59ef1b99497a76dcd4507322b5bfb1d4;
        itemHashes[
            5
        ] = 0xb47a207d81a86011ba7f56631dc651db9b280a76406097b3c1b42b09ecda5a2e;
        itemHashes[
            6
        ] = 0x3492cc0bf469717d0a6b72e9fe80c17d274042c80d7180b78d61c80ab078ac9e;

        bytes32 root = calculateRootPaths(
            startPath,
            endPath,
            startIndex,
            itemHashes
        );
        console.logBytes32(root);
    }
}
