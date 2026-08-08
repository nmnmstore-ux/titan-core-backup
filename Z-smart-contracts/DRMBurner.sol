// ============================================================
// THE-BRIDGE DRM Burner - Deflationary Mechanism
// Sovereign Master Prompt: 10% of USD fees for Buy-back and Burn
// ============================================================

// SPDX-License-Identifier: THE-Bridge
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

interface IDRMToken is IERC20 {
    function burn(uint256 amount) external;
}

// Mock Oracle for USD/DRM price. In production, use Chainlink or similar.
interface IMockPriceOracle {
    function getLatestPrice(string calldata pair) external view returns (uint256);
}

contract DRMBurner is Ownable {
    IDRMToken public immutable drmToken;
    IMockPriceOracle public immutable usdDrmPriceOracle;

    event DRMBurned(uint256 usdAmount, uint256 drmAmount);

    address public burnerServiceAddress;

    constructor(address _drmToken, address _usdDrmPriceOracle, address _burnerServiceAddress) Ownable(msg.sender) {
        require(_drmToken != address(0), "Burner: Invalid DRM token address");
        require(_usdDrmPriceOracle != address(0), "Burner: Invalid Oracle address");
        require(_burnerServiceAddress != address(0), "Burner: Invalid burner service address");

        drmToken = IDRMToken(_drmToken);
        usdDrmPriceOracle = IMockPriceOracle(_usdDrmPriceOracle);
        burnerServiceAddress = _burnerServiceAddress;
    }

    function setBurnerServiceAddress(address _newAddress) public onlyOwner {
        require(_newAddress != address(0), "Burner: Invalid new address");
        burnerServiceAddress = _newAddress;
    }

    function burnDRMFromFees(uint256 usdAmount, uint256 drmAmountToBurn) external {
        require(msg.sender == burnerServiceAddress, "Burner: Only authorized service");
        require(usdAmount > 0, "Burner: USD amount must be > 0");
        require(drmAmountToBurn > 0, "Burner: DRM amount to burn must be > 0");

        drmToken.burn(drmAmountToBurn);

        emit DRMBurned(usdAmount, drmAmountToBurn);
    }

    receive() external payable {}
}