// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script, console} from "forge-std/Script.sol";
import {Counter} from "../src/Counter.sol";
import {L1VerifierPlonk} from "../src/L1VerifierPlonk.sol";
import {L1VerifierFflonk} from "../src/L1VerifierFflonk.sol";
import {DualVerifier} from "../src/DualVerifier.sol";

contract CounterScript is Script {
    Counter public counter;
    L1VerifierPlonk public l1VerifierPlonk;
    L1VerifierFflonk public l1VerifierFflonk;
    DualVerifier public dualVerifier;

    function setUp() public {}

    function run() public {
        vm.startBroadcast();

        counter = new Counter();
        l1VerifierPlonk = new L1VerifierPlonk();
        l1VerifierFflonk = new L1VerifierFflonk();
        dualVerifier = new DualVerifier(l1VerifierFflonk, l1VerifierPlonk);
        console.log("Dual verifier address:", address(dualVerifier));
        console.log("Plonk verifier address:", address(l1VerifierPlonk));

        vm.stopBroadcast();
    }
}
