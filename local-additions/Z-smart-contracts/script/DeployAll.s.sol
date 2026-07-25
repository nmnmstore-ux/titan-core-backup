// SPDX-License-Identifier: SWIFTBRIDGE
pragma solidity ^0.8.24;

import "forge-std/Script.sol";
import "../SwiftBridgeDAO.sol";
import "../DRS.sol";
import "../UnilateralRecovery.sol";
import "../RWA.sol";
import "../Token.sol";

contract DeployAll is Script {
    function run() external {
        uint256 deployerKey = vm.envUint("DEPLOYER_PRIVATE_KEY");
        address deployer = vm.addr(deployerKey);
        address sgxVerifier = vm.envAddress("SGX_VERIFIER_ADDRESS");
        address teeEnclave = vm.envAddress("TEE_ENCLAVE_ADDRESS");

        vm.startBroadcast(deployerKey);

        // 1. Deploy SWB Token
        SWBToken token = new SWBToken();
        console.log("SWBToken deployed at:", address(token));

        // 2. Deploy DAO
        SwiftBridgeDAO dao = new SwiftBridgeDAO(address(token), teeEnclave);
        console.log("SwiftBridgeDAO deployed at:", address(dao));

        // 3. Deploy DRS
        DynamicRebateSystem drs = new DynamicRebateSystem(address(token));
        console.log("DynamicRebateSystem deployed at:", address(drs));

        // 4. Deploy Unilateral Recovery (v2 with balance caps)
        UnilateralRecovery ur = new UnilateralRecovery(address(token), address(dao), sgxVerifier, teeEnclave);
        console.log("UnilateralRecovery deployed at:", address(ur));

        // 5. Deploy RWA Backing
        RWABacking rwa = new RWABacking();
        console.log("RWABacking deployed at:", address(rwa));

        // === POST-DEPLOY SETUP ===
        // Transfer DAO ownership to DAO contract
        token.transferOwnership(address(dao));
        console.log("Token ownership transferred to DAO");

        // Add initial reserves
        rwa.addReserve("USDC", 10_000_000, 10_000_000_000_000, address(0), keccak256("genesis"));

        // Fund DRS rebate pool
        token.mint(address(drs), 5_000_000 * 10**18);

        // Fund UnilateralRecovery reserve
        token.mint(address(ur), 1_000_000 * 10**18);

        vm.stopBroadcast();

        // Output deployment summary
        console.log("=== DEPLOYMENT COMPLETE ===");
        console.log("Network:", block.chainid == 11155111 ? "Sepolia" : block.chainid == 5 ? "Goerli" : "Unknown");
        console.log("Block:", block.number);
        console.log("SWB Total Supply:", token.totalSupply());
        console.log("DAO Halted:", dao.isHalted());
    }
}
