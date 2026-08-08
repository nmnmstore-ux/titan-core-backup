// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "forge-std/Script.sol";
import "forge-std/console.sol";
import "../FlashLoanArbitrage.sol";

/// Deploys the FlashLoanArbitrage executor on Sepolia.
///
/// Env config:
///   DEPLOYER_PRIVATE_KEY  (required)
///   RPC_URL               full provider URL for --rpc-url override
///   POOL_ADDRESS           (default Aave V3 Sepolia pool)
///   SWAP_ROUTER            (default Uniswap UniversalRouter Sepolia)
contract DeployFlashLoanArbitrage is Script {
    address constant AAVE_SEPOLIA = 0x6Ae43d3271ff6888e7Fc43Fd7321a503ff738951;
    address constant UNIVERSAL_ROUTER_SEPOLIA = 0x3fC91A3afd70395Cd496C647d5a6CC9D4B2b7FAD;

    function run() external returns (address deployed) {
        uint256 pk = vm.envUint("DEPLOYER_PRIVATE_KEY");
        address poolAddr = vm.envOr("POOL_ADDRESS", AAVE_SEPOLIA);
        address router = vm.envOr("SWAP_ROUTER", UNIVERSAL_ROUTER_SEPOLIA);

        vm.startBroadcast(pk);
        FlashLoanArbitrage arb = new FlashLoanArbitrage(poolAddr, router);
        vm.stopBroadcast();

        address a = address(arb);
        console.log("FlashLoanArbitrage deployed at:", a);
        console.log("Aave V3 Pool (Sepolia):", poolAddr);
        console.log("UniversalRouter (Sepolia):", router);
        return a;
    }
}
