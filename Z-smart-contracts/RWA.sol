// ============================================================
// THE-Bridge RWA - Real World Asset Backing
// Sovereign Master Prompt: كل وحدة قيمة لها غطاء حقيقي
// ذهب - OZ stablecoins - أصول حقيقية
// ============================================================

// SPDX-License-Identifier: THE-Bridge
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/access/AccessControl.sol";

contract RWABacking is ERC20, AccessControl {
    bytes32 public constant AUDITOR_ROLE = keccak256("AUDITOR_ROLE");
    bytes32 public constant MINT_AUTHORITY = keccak256("MINT_AUTHORITY");

    uint256 public totalReserves;    // USD value of physical reserves
    uint256 public reserveRatio;     // Current reserve ratio (basis points, 11000 = 110%)
    uint256 public immutable MIN_RESERVE_RATIO = 11000; // 110%

    mapping(address => uint256) public assetReserves; // Asset type → amount
    mapping(bytes32 => AuditProof) public auditTrail;

    struct AssetReserve {
        string assetType; // GOLD, OZ_STABLECOIN, TREASURY, BANK_BALANCE
        uint256 amount;
        uint256 usdValue;
        address custodian;
        uint256 lastAudited;
        bytes32 proofHash;
    }

    struct AuditProof {
        uint256 timestamp;
        uint256 totalReserves;
        uint256 totalSupply;
        uint256 ratio;
        bytes32 merkleRoot;
        string auditor;
    }

    AssetReserve[] public reserves;

    event ReserveAdded(string assetType, uint256 amount, uint256 usdValue, address custodian);
    event AuditCompleted(uint256 timestamp, uint256 ratio, string auditor);
    event ReserveRatioUpdated(uint256 newRatio);

    constructor() ERC20("THE-Bridge RWA Backed", "DRMRWA") {
        _grantRole(DEFAULT_ADMIN_ROLE, msg.sender);
        _grantRole(AUDITOR_ROLE, msg.sender);
        _grantRole(MINT_AUTHORITY, msg.sender);
        reserveRatio = MIN_RESERVE_RATIO;
    }

    // Add physical reserve
    function addReserve(
        string calldata assetType,
        uint256 amount,
        uint256 usdValue,
        address custodian,
        bytes32 proofHash
    ) external onlyRole(AUDITOR_ROLE) {
        reserves.push(AssetReserve({
            assetType: assetType,
            amount: amount,
            usdValue: usdValue,
            custodian: custodian,
            lastAudited: block.timestamp,
            proofHash: proofHash
        }));

        totalReserves += usdValue;
        _updateReserveRatio();

        emit ReserveAdded(assetType, amount, usdValue, custodian);
    }

    // Mint backed tokens
    function mintBacked(address to, uint256 amount) external onlyRole(MINT_AUTHORITY) {
        uint256 newSupply = totalSupply() + amount;
        require(newSupply <= totalReserves, "RWA: insufficient reserves");

        // Check reserve ratio after mint
        uint256 newRatio = (totalReserves * 10000) / newSupply;
        require(newRatio >= MIN_RESERVE_RATIO, "RWA: would violate min reserve ratio");

        _mint(to, amount);
        _updateReserveRatio();
    }

    // Burn tokens (redeem for underlying)
    function burnBacked(uint256 amount) external {
        _burn(msg.sender, amount);
        _updateReserveRatio();
    }

    // Complete audit
    function completeAudit(bytes32 merkleRoot, string calldata auditor) external onlyRole(AUDITOR_ROLE) {
        uint256 ts = block.timestamp;
        auditTrail[keccak256(abi.encodePacked(ts, auditor))] = AuditProof({
            timestamp: ts,
            totalReserves: totalReserves,
            totalSupply: totalSupply(),
            ratio: reserveRatio,
            merkleRoot: merkleRoot,
            auditor: auditor
        });

        emit AuditCompleted(ts, reserveRatio, auditor);
    }

    function _updateReserveRatio() internal {
        uint256 supply = totalSupply();
        if (supply > 0) {
            reserveRatio = (totalReserves * 10000) / supply;
        } else {
            reserveRatio = MIN_RESERVE_RATIO;
        }
        emit ReserveRatioUpdated(reserveRatio);
    }

    function getReserveCount() external view returns (uint256) {
        return reserves.length;
    }

    function isSolvent() external view returns (bool) {
        return reserveRatio >= MIN_RESERVE_RATIO;
    }
}
