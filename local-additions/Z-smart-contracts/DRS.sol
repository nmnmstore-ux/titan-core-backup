// ============================================================
// SwiftBridge DRS - Dynamic Rebate System
// Sovereign Master Prompt: 30-40% من الرسوم كمكافآت نمو
// ============================================================

// SPDX-License-Identifier: SWIFTBRIDGE
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";

contract DynamicRebateSystem {
    IERC20 public immutable swbToken;

    uint256 public constant REBATE_POOL_PCT = 35; // 35% of fees → rebates
    uint256 public constant TIER_1_THRESHOLD = 10_000 * 10**18;  // 10K SWB staked
    uint256 public constant TIER_2_THRESHOLD = 50_000 * 10**18;  // 50K SWB
    uint256 public constant TIER_3_THRESHOLD = 250_000 * 10**18; // 250K SWB

    mapping(address => uint256) public rebates;
    mapping(address => uint256) public totalVolume;
    mapping(address => uint256) public staked;
    uint256 public totalRebatesDistributed;

    event RebateClaimed(address indexed user, uint256 amount, uint8 tier);
    event VolumeRecorded(address indexed user, uint256 volume);

    constructor(address _swbToken) {
        swbToken = IERC20(_swbToken);
    }

    // Record user volume → calculate rebate
    function recordVolume(address user, uint256 volume, uint256 fee) external {
        totalVolume[user] += volume;
        uint256 rebate = (fee * REBATE_POOL_PCT) / 100;

        // Tier multiplier
        uint256 tierMultiplier = getTierMultiplier(user);
        rebate = (rebate * tierMultiplier) / 100;

        rebates[user] += rebate;
        totalRebatesDistributed += rebate;

        emit VolumeRecorded(user, volume);
    }

    function claimRebate() external {
        uint256 amount = rebates[msg.sender];
        require(amount > 0, "DRS: no rebate available");

        rebates[msg.sender] = 0;
        uint8 tier = getUserTier(msg.sender);
        require(swbToken.transfer(msg.sender, amount), "DRS: transfer failed");

        emit RebateClaimed(msg.sender, amount, tier);
    }

    function stake(uint256 amount) external {
        require(swbToken.transferFrom(msg.sender, address(this), amount), "DRS: stake failed");
        staked[msg.sender] += amount;
    }

    function unstake(uint256 amount) external {
        require(staked[msg.sender] >= amount, "DRS: insufficient stake");
        staked[msg.sender] -= amount;
        require(swbToken.transfer(msg.sender, amount), "DRS: unstake failed");
    }

    function getTierMultiplier(address user) public view returns (uint256) {
        uint256 s = staked[user];
        if (s >= TIER_3_THRESHOLD) return 200; // 2x rebate
        if (s >= TIER_2_THRESHOLD) return 150; // 1.5x rebate
        if (s >= TIER_1_THRESHOLD) return 120; // 1.2x rebate
        return 100; // 1x rebate
    }

    function getUserTier(address user) public view returns (uint8) {
        uint256 s = staked[user];
        if (s >= TIER_3_THRESHOLD) return 3;
        if (s >= TIER_2_THRESHOLD) return 2;
        if (s >= TIER_1_THRESHOLD) return 1;
        return 0;
    }

    function getRebate(address user) external view returns (uint256) {
        return rebates[user];
    }
}
