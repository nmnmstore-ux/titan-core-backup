// ============================================================
// THE-BRIDGE Digital Reserve Money (DRM) - Native Utility & Governance
// Sovereign Master Prompt: 1 Billion Fixed Supply, No Minting after launch
// ============================================================

// SPDX-License-Identifier: THE-Bridge
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/token/ERC20/extensions/ERC20Burnable.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

contract DRMToken is ERC20, ERC20Burnable, Ownable {
    uint256 public constant MAX_SUPPLY = 1_000_000_000 * 10**18; // 1 Billion DRM

    event InitialSupplyMinted(address indexed recipient, uint256 amount);

    constructor(address initialRecipient) ERC20("Digital Reserve Money", "DRM") Ownable(msg.sender) {
        _mint(initialRecipient, MAX_SUPPLY);
        emit InitialSupplyMinted(initialRecipient, MAX_SUPPLY);
    }
}