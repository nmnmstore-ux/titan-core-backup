// SPDX-License-Identifier: SWIFTBRIDGE
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/token/ERC20/extensions/ERC20Burnable.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

contract SWBToken is ERC20, ERC20Burnable, Ownable {
    uint256 public constant MAX_SUPPLY = 10_000_000 * 10**18;

    event InitialSupplyMinted(address indexed recipient, uint256 amount);

    constructor() ERC20("SwiftBridge", "SWB") Ownable(msg.sender) {
        _mint(msg.sender, MAX_SUPPLY);
        emit InitialSupplyMinted(msg.sender, MAX_SUPPLY);
    }

    function mint(address to, uint256 amount) external onlyOwner {
        require(totalSupply() + amount <= MAX_SUPPLY, "SWB: max supply reached");
        _mint(to, amount);
    }
}