// ============================================================
// SwiftBridge USB Burner - Deflationary Mechanism
// Sovereign Master Prompt: 10% of USD fees for Buy-back and Burn
// ============================================================

// SPDX-License-Identifier: SWIFTBRIDGE
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

interface IUSBToken is IERC20 {
    function burn(uint256 amount) external;
}

// Mock Oracle for USD/USB price. In production, use Chainlink or similar.
interface IMockPriceOracle {
    function getLatestPrice(string calldata pair) external view returns (uint256);
}

contract USBBurner is Ownable {
    IUSBToken public immutable usbToken;
    IMockPriceOracle public immutable usdUsbPriceOracle;

    // Event to log burn operations
    event USBBurned(uint256 usdAmount, uint256 usbAmount);

    // Only an authorized backend service can trigger buy-back and burn
    address public burnerServiceAddress;

    constructor(address _usbToken, address _usdUsbPriceOracle, address _burnerServiceAddress) Ownable(msg.sender) {
        require(_usbToken != address(0), "Burner: Invalid USB token address");
        require(_usdUsbPriceOracle != address(0), "Burner: Invalid Oracle address");
        require(_burnerServiceAddress != address(0), "Burner: Invalid burner service address");

        usbToken = IUSBToken(_usbToken);
        usdUsbPriceOracle = IMockPriceOracle(_usdUsbPriceOracle);
        burnerServiceAddress = _burnerServiceAddress;
    }

    // Function to update the burner service address (only by owner/DAO)
    function setBurnerServiceAddress(address _newAddress) public onlyOwner {
        require(_newAddress != address(0), "Burner: Invalid new address");
        burnerServiceAddress = _newAddress;
    }

    // Main function to receive USD fees and burn USB
    // This function assumes `msg.sender` has already swapped USD fees for USB tokens
    // or has enough USB tokens to burn corresponding to `usdAmount`.
    function burnUSBFromFees(uint256 usdAmount, uint256 usbAmountToBurn) external {
        require(msg.sender == burnerServiceAddress, "Burner: Only authorized service");
        require(usdAmount > 0, "Burner: USD amount must be > 0");
        require(usbAmountToBurn > 0, "Burner: USB amount to burn must be > 0");

        // Transfer USB from burner service to this contract before burning (if not already held)
        // This assumes the burnerServiceAddress handles the swap from USD to USB.
        // For simplicity, we assume burnerServiceAddress already holds usbAmountToBurn or sends it with this tx.
        // In a real scenario, the burnerService would likely approve this contract to spend USB.

        // The burner service should ensure it has enough USB tokens to burn.
        // We'll directly call burn on the USBToken contract here.
        usbToken.burn(usbAmountToBurn);

        emit USBBurned(usdAmount, usbAmountToBurn);
    }

    // Fallback function to receive ETH (or any token via explicit transfer) if needed
    receive() external payable {}
}
