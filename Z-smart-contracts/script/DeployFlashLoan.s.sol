// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "forge-std/Script.sol";
import "forge-std/console.sol";
import "../FlashLoanArbitrage.sol";

contract DeployFlashLoan is Script {
    function run() external {
        uint256 pk = vm.envUint("DEPLOYER_PRIVATE_KEY");
        address poolAddr = address(0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2);
        address routerAddr = address(0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45);

        vm.startBroadcast(pk);
        FlashLoanArbitrage arb = new FlashLoanArbitrage(poolAddr, routerAddr);
        vm.stopBroadcast();

        console.log("FlashLoanArbitrage deployed at:", address(arb));
    }
}