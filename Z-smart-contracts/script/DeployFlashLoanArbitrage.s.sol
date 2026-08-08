// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "forge-std/Script.sol";
import "forge-std/console.sol";
import "../FlashLoanArbitrage.sol";

/// Deploys the FlashLoanArbitrage executor.
///
/// Env config:
///   DEPLOYER_PRIVATE_KEY  (required)
///   RPC_URL               full provider URL for --rpc-url override
///   POOL_ADDRESS           (default Aave V3 mainnet pool)
///   SWAP_ROUTER            (default Uniswap V3 SwapRouter02 mainnet)
contract DeployFlashLoanArbitrage is Script {
    address constant AAVE_MAINNET = 0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2;
    address constant UNIV3_ROUTER_MAINNET = 0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45;

    function run() external returns (address deployed) {
        uint256 pk = vm.envUint("DEPLOYER_PRIVATE_KEY");
        address poolAddr = vm.envOr("POOL_ADDRESS", AAVE_MAINNET);
        address router = vm.envOr("SWAP_ROUTER", UNIV3_ROUTER_MAINNET);

        vm.startBroadcast(pk);
        FlashLoanArbitrage arb = new FlashLoanArbitrage(poolAddr, router);
        vm.stopBroadcast();

        address a = address(arb);
        console.log("FlashLoanArbitrage deployed at:", a);
        console.log("Aave V3 Pool:", poolAddr);
        console.log("SwapRouter:", router);
        return a;
    }
}