// ============================================================
// SwiftBridge Mock Price Oracle - For Testing Only
// ============================================================

// SPDX-License-Identifier: SWIFTBRIDGE
pragma solidity ^0.8.24;

contract MockPriceOracle {
    mapping(string => uint256) public prices;

    constructor() {
        // Set initial prices for testing
        prices["USD/USB"] = 100 * (10**18); // 1 USB = $100 (example)
        prices["USB/USD"] = 1 * (10**18) / 100; // $1 = 0.01 USB
        prices["ETH/USD"] = 3000 * (10**18);
        prices["BTC/USD"] = 60000 * (10**18);
    }

    function setPrice(string calldata pair, uint256 price) external {
        prices[pair] = price;
    }

    function getLatestPrice(string calldata pair) external view returns (uint256) {
        require(prices[pair] > 0, "Oracle: Price not set");
        return prices[pair];
    }
}
