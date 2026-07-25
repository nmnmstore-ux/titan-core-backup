// ============================================================
// SwiftBridge USB Token - Native Utility & Governance
// Sovereign Master Prompt: 1 Billion Fixed Supply, No Minting after launch
// ============================================================

// SPDX-License-Identifier: SWIFTBRIDGE
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/token/ERC20/extensions/ERC20Burnable.sol";
import "@openzeppelin/contracts/access/Ownable.sol"; // Using Ownable for initial control before DAO

contract USBToken is ERC20, ERC20Burnable, Ownable {
    uint256 public constant MAX_SUPPLY = 1_000_000_000 * 10**18; // 1 Billion USB

    // Event to log initial supply transfer to DAO (or deployer for temporary control)
    event InitialSupplyMinted(address indexed recipient, uint256 amount);

    constructor(address initialRecipient) ERC20("Unified Swift-Bridge", "USB") Ownable(msg.sender) {
        // Mint all tokens in constructor. No further minting allowed.
        _mint(initialRecipient, MAX_SUPPLY);
        emit InitialSupplyMinted(initialRecipient, MAX_SUPPLY);
    }

    // No mint function - fixed supply

    // Optional: Only owner (temporarily deployer, then DAO) can pause/unpause
    // function pause() public onlyOwner { _pause(); }
    // function unpause() public onlyOwner { _unpause(); }

    // Add burn function from ERC20Burnable
}
