// SPDX-License-Identifier: MIT

pragma solidity 0.8.28;
import {console} from "forge-std/Test.sol";

/// @notice Part of the configuration parameters of ZKP circuits
struct VerifierParams {
    bytes32 recursionNodeLevelVkHash;
    bytes32 recursionLeafLevelVkHash;
    bytes32 recursionCircuitsSetVksHash;
}

/// @title The interface of the Verifier contract, responsible for the zero knowledge proof verification.
/// @author Matter Labs
/// @custom:security-contact security@matterlabs.dev
interface IVerifier {
    /// @dev Verifies a zk-SNARK proof.
    /// @return A boolean value indicating whether the zk-SNARK proof is valid.
    /// Note: The function may revert execution instead of returning false in some cases.
    function verify(
        uint256[] calldata _publicInputs,
        uint256[] calldata _proof
    ) external view returns (bool);

    /// @notice Calculates a keccak256 hash of the runtime loaded verification keys.
    /// @return vkHash The keccak256 hash of the loaded verification keys.
    function verificationKeyHash() external view returns (bytes32);
}

interface IVerifierV2 {
    /// @dev Verifies a zk-SNARK proof.
    /// @return A boolean value indicating whether the zk-SNARK proof is valid.
    /// Note: The function may revert execution instead of returning false in some cases.
    function verify(
        uint256[] calldata _publicInputs,
        uint256[] calldata _proof
    ) external view returns (bool);

    /// @notice Calculates a keccak256 hash of the runtime loaded verification keys.
    /// @return vkHash The keccak256 hash of the loaded verification keys.
    function verificationKeyHash() external view returns (bytes32);
}

// 0xc352bb73
error UnknownVerifierType();
// 0x456f8f7a
error EmptyProofLength();

// 0xd08a97e6
error InvalidMockProofLength();
// 0x09bde339
error InvalidProof();

/// @title Dual Verifier
/// @author Matter Labs
/// @custom:security-contact security@matterlabs.dev
/// @notice This contract wraps two different verifiers (FFLONK and PLONK) and routes zk-SNARK proof verification
/// to the correct verifier based on the provided proof type. It reuses the same interface as on the original `Verifier`
/// contract, while abusing on of the fields (`_recursiveAggregationInput`) for proof verification type. The contract is
/// needed for the smooth transition from PLONK based verifier to the FFLONK verifier.
contract DualVerifier is IVerifier {
    /// @notice The latest FFLONK verifier contract.
    IVerifierV2 public immutable FFLONK_VERIFIER;

    /// @notice PLONK verifier contract.
    IVerifier public immutable PLONK_VERIFIER;

    /// @notice Type of verification for FFLONK verifier.
    uint256 internal constant FFLONK_VERIFICATION_TYPE = 0;

    /// @notice Type of verification for PLONK verifier.
    uint256 internal constant PLONK_VERIFICATION_TYPE = 1;

    uint256 internal constant OHBENDER_PLONK_VERIFICATION_TYPE = 2;

    // @notice Code must be removed before prod.
    uint256 internal constant OHBENDER_MOCK_VERIFICATION_TYPE = 3;

    /// @param _fflonkVerifier The address of the FFLONK verifier contract.
    /// @param _plonkVerifier The address of the PLONK verifier contract.
    constructor(IVerifierV2 _fflonkVerifier, IVerifier _plonkVerifier) {
        FFLONK_VERIFIER = _fflonkVerifier;
        PLONK_VERIFIER = _plonkVerifier;
    }

    /// @notice Routes zk-SNARK proof verification to the appropriate verifier (FFLONK or PLONK) based on the proof type.
    /// @param _publicInputs The public inputs to the proof.
    /// @param _proof The zk-SNARK proof itself.
    /// @dev  The first element of the `_proof` determines the verifier type.
    ///     - 0 indicates the FFLONK verifier should be used.
    ///     - 1 indicates the PLONK verifier should be used.
    /// @return Returns `true` if the proof verification succeeds, otherwise throws an error.
    function verify(
        uint256[] calldata _publicInputs,
        uint256[] calldata _proof
    ) public view virtual returns (bool) {
        // Ensure the proof has a valid length (at least one element
        // for the proof system differentiator).
        if (_proof.length == 0) {
            revert EmptyProofLength();
        }

        // The first element of `_proof` determines the verifier type (either FFLONK or PLONK).
        uint256 verifierType = _proof[0];
        if (verifierType == FFLONK_VERIFICATION_TYPE) {
            return FFLONK_VERIFIER.verify(_publicInputs, _extractProof(_proof));
        } else if (verifierType == PLONK_VERIFICATION_TYPE) {
            return PLONK_VERIFIER.verify(_publicInputs, _extractProof(_proof));
        } else if (verifierType == OHBENDER_PLONK_VERIFICATION_TYPE) {
            uint256[] memory args = new uint256[](1);
            args[0] = computeOhBenderHash(_proof[1], _publicInputs);

            console.logBytes32(bytes32(args[0]));

            return PLONK_VERIFIER.verify(args, _extractOhBenderProof(_proof));
        } else if (verifierType == OHBENDER_MOCK_VERIFICATION_TYPE) {
            uint256[] memory args = new uint256[](1);
            args[0] = computeOhBenderHash(_proof[1], _publicInputs);

            console.logBytes32(bytes32(args[0]));

            return mockverify(args, _extractOhBenderProof(_proof));
        }
        // If the verifier type is unknown, revert with an error.
        else {
            revert UnknownVerifierType();
        }
    }

    function mockverify(
        uint256[] memory _publicInputs,
        uint256[] memory _proof
    ) public view virtual returns (bool) {
        if (_proof.length != 2) {
            revert InvalidMockProofLength();
        }
        if (_proof[0] != 13) {
            revert InvalidProof();
        }
        if (_proof[1] != _publicInputs[0]) {
            revert InvalidProof();
        }
        return true;
    }

    /// @inheritdoc IVerifier
    /// @dev Used for backward compatibility with older Verifier implementation. Returns PLONK verification key hash.
    function verificationKeyHash() external view returns (bytes32) {
        return PLONK_VERIFIER.verificationKeyHash();
    }

    /// @notice Calculates a keccak256 hash of the runtime loaded verification keys from the selected verifier.
    /// @return The keccak256 hash of the loaded verification keys based on the verifier.
    function verificationKeyHash(
        uint256 _verifierType
    ) external view returns (bytes32) {
        if (_verifierType == FFLONK_VERIFICATION_TYPE) {
            return FFLONK_VERIFIER.verificationKeyHash();
        } else if (_verifierType == PLONK_VERIFICATION_TYPE) {
            return PLONK_VERIFIER.verificationKeyHash();
        }
        // If the verifier type is unknown, revert with an error.
        else {
            revert UnknownVerifierType();
        }
    }

    /// @notice Extract the proof by removing the first element (proof type differentiator).
    /// @param _proof The proof array array.
    /// @return result A new array with the first element removed. The first element was used as a hack for
    /// differentiator between FFLONK and PLONK proofs.
    function _extractProof(
        uint256[] calldata _proof
    ) internal pure returns (uint256[] memory result) {
        uint256 resultLength = _proof.length - 1;

        // Allocate memory for the new array (_proof.length - 1) since the first element is omitted.
        result = new uint256[](resultLength);

        // Copy elements starting from index 1 (the second element) of the original array.
        assembly {
            calldatacopy(
                add(result, 0x20),
                add(_proof.offset, 0x20),
                mul(resultLength, 0x20)
            )
        }
    }

    function _extractOhBenderProof(
        uint256[] calldata _proof
    ) internal pure returns (uint256[] memory result) {
        uint256 resultLength = _proof.length - 1 - 1;

        // Allocate memory for the new array (_proof.length - 1) since the first element is omitted.
        result = new uint256[](resultLength);

        // Copy elements starting from index 1 (the second element) of the original array.
        assembly {
            calldatacopy(
                add(result, 0x20),
                add(_proof.offset, 0x40),
                mul(resultLength, 0x20)
            )
        }
    }

    function reverseUint256(
        uint256 input
    ) internal pure returns (uint256 result) {
        bytes memory out = new bytes(32);

        for (uint256 i = 0; i < 8; i++) {
            uint32 chunk = uint32(input >> (32 * (7 - i))); // get 4-byte chunk
            // Reverse 4 bytes of the chunk
            out[i * 4 + 0] = bytes1(uint8(chunk >> 0));
            out[i * 4 + 1] = bytes1(uint8(chunk >> 8));
            out[i * 4 + 2] = bytes1(uint8(chunk >> 16));
            out[i * 4 + 3] = bytes1(uint8(chunk >> 24));
        }

        assembly {
            result := mload(add(out, 32))
        }
    }

    /// Temporary thing.
    function keccakTwoUint256(
        uint256 a,
        uint256 b
    ) public pure returns (uint256) {
        uint256 a_be = reverseUint256(a);
        uint256 b_be = reverseUint256(b);
        bytes32 hash = keccak256(abi.encodePacked(a_be, b_be));
        return reverseUint256(uint256(hash));
    }

    function computeOhBenderHash(
        uint256 initialHash,
        uint256[] calldata _publicInputs
    ) public pure returns (uint256 result) {
        if (initialHash == 0) {
            initialHash = _publicInputs[0];
            for (uint256 i = 1; i < _publicInputs.length; i++) {
                initialHash = keccakTwoUint256(initialHash, _publicInputs[i]);
                /*uint256(
                    keccak256(abi.encodePacked(initialHash, _publicInputs[i]))
                );*/
            }
        } else {
            for (uint256 i = 0; i < _publicInputs.length; i++) {
                initialHash = keccakTwoUint256(initialHash, _publicInputs[i]);
                /*initialHash = uint256(
                    keccak256(abi.encodePacked(initialHash, _publicInputs[i]))
                );*/
            }
        }

        result = initialHash;
    }
}
