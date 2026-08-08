// ============================================================
// THE-Bridge Unilateral Recovery v2
// Sovereign Master Prompt: استرداد ذاتي حتى لو توقف البروتوكول
// المستخدم يسحب ماله يدوياً بدون موافقة أحد
//
// Fixes applied (AUDIT-REPORT.md):
//  BUG #4 — Signature replay via EIP-712 (chainid + contract + nonce)
//  BUG #5 — SGX quote verification via ISGXVerifier
//  BUG #6 — Deadline buffer + block.timestamp tolerance
//  BUG #7 — Nonce in recovery requests
//  ADDED — Balance cap: recovery amount ≤ user's protocol balance
// ============================================================

// SPDX-License-Identifier: SWIFTBRIDGE
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import "@openzeppelin/contracts/utils/cryptography/SignatureChecker.sol";
import "@openzeppelin/contracts/utils/structs/EnumerableSet.sol";

interface ISGXVerifier {
    function verifyQuote(bytes calldata quote, bytes calldata signature) external view returns (bool);
}

contract UnilateralRecovery {
    using ECDSA for bytes32;
    using EnumerableSet for EnumerableSet.AddressSet;

    bytes32 private immutable DOMAIN_SEPARATOR;
    bytes32 public constant RECOVERY_TYPEHASH = keccak256(
        "RecoveryRequest(address user,uint256 amount,uint256 nonce,uint256 deadline)"
    );

    address public immutable drmToken;
    address public immutable dao;
    ISGXVerifier public immutable sgxVerifier;
    address public immutable teeEnclave;
    uint256 public constant RECOVERY_TIMELOCK = 30 days;
    uint256 public constant MAX_RECOVERY_PCT = 95;
    uint256 public constant DEADLINE_BUFFER = 3600; // 1 hour tolerance for block timestamp drift

    struct RecoveryRequest {
        address user;
        uint256 amount;
        uint256 nonce;
        uint256 deadline;
        bool executed;
        bytes signature;
    }

    mapping(address => RecoveryRequest) public requests;
    mapping(address => uint256) public lastRecovery;
    mapping(address => uint256) public userBalance;
    mapping(address => uint256) private nonces;
    EnumerableSet.AddressSet private registeredUsers;

    uint256 public totalRecovered;
    uint256 public totalUsers;

    event RecoveryRequested(address indexed user, uint256 amount, uint256 deadline);
    event RecoveryExecuted(address indexed user, uint256 amount);
    event RecoveryCanceled(address indexed user);
    event BalanceSet(address indexed user, uint256 amount);
    event EmergencyWithdrawal(address indexed user, uint256 amount, bytes32 quoteHash);

    constructor(address _drmToken, address _dao, address _sgxVerifier, address _teeEnclave) {
        drmToken = _drmToken;
        dao = _dao;
        sgxVerifier = ISGXVerifier(_sgxVerifier);
        teeEnclave = _teeEnclave;

        DOMAIN_SEPARATOR = keccak256(
            abi.encode(
                keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"),
                keccak256("THE-Bridge Unilateral Recovery"),
                keccak256("2"),
                block.chainid,
                address(this)
            )
        );
    }

    /// DAO sets a user's recoverable balance
    function setBalance(address user, uint256 amount) external {
        require(msg.sender == dao, "UR: only DAO");
        userBalance[user] = amount;
        if (!registeredUsers.contains(user)) {
            registeredUsers.add(user);
            totalUsers++;
        }
        emit BalanceSet(user, amount);
    }

    /// Batch set balances (for initial migration)
    function setBalances(address[] calldata users, uint256[] calldata amounts) external {
        require(msg.sender == dao, "UR: only DAO");
        require(users.length == amounts.length, "UR: length mismatch");
        for (uint256 i = 0; i < users.length; i++) {
            userBalance[users[i]] = amounts[i];
            if (!registeredUsers.contains(users[i])) {
                registeredUsers.add(users[i]);
                totalUsers++;
            }
            emit BalanceSet(users[i], amounts[i]);
        }
    }

    /// Request unilateral withdrawal using EIP-712 typed signature
    function requestRecovery(uint256 amount, uint256 deadline, bytes calldata signature) external {
        require(amount > 0, "UR: amount must be >0");
        require(amount <= userBalance[msg.sender], "UR: exceeds balance");
        require(deadline > block.timestamp, "UR: deadline expired");
        require(deadline <= block.timestamp + DEADLINE_BUFFER, "UR: deadline too far");
        require(block.timestamp >= lastRecovery[msg.sender] + RECOVERY_TIMELOCK, "UR: cooldown active");

        uint256 currentNonce = nonces[msg.sender];
        bytes32 digest = _hashTypedDataV4(keccak256(abi.encode(
            RECOVERY_TYPEHASH,
            msg.sender,
            amount,
            currentNonce,
            deadline
        )));

        address signer = digest.recover(signature);
        require(signer == msg.sender, "UR: invalid signature");
        require(signer != address(0), "UR: invalid signer");

        nonces[msg.sender]++;

        requests[msg.sender] = RecoveryRequest({
            user: msg.sender,
            amount: amount,
            nonce: currentNonce,
            deadline: deadline,
            executed: false,
            signature: signature
        });

        emit RecoveryRequested(msg.sender, amount, deadline);
    }

    /// Execute recovery after timelock (anyone can trigger)
    function executeRecovery(address user) external {
        RecoveryRequest storage req = requests[user];
        require(req.amount > 0, "UR: no request");
        require(!req.executed, "UR: already executed");
        require(block.timestamp >= req.deadline, "UR: deadline not met");
        require(block.timestamp >= req.deadline + RECOVERY_TIMELOCK, "UR: timelock not expired");

        req.executed = true;
        lastRecovery[user] = block.timestamp;

        uint256 cappedAmount = req.amount <= userBalance[user] ? req.amount : userBalance[user];
        uint256 actualAmount = (cappedAmount * MAX_RECOVERY_PCT) / 100;
        require(actualAmount > 0, "UR: nothing to recover");

        // Reduce user's on-chain balance
        userBalance[user] = userBalance[user] > actualAmount ? userBalance[user] - actualAmount : 0;
        totalRecovered += actualAmount;

        require(IERC20(drmToken).transfer(user, actualAmount), "UR: transfer failed");

        emit RecoveryExecuted(user, actualAmount);
    }

    /// Cancel recovery request
    function cancelRecovery() external {
        delete requests[msg.sender];
        nonces[msg.sender]++;
        emit RecoveryCanceled(msg.sender);
    }

    /// Emergency withdrawal by TEE (SGX-attested)
    function teeEmergencyWithdraw(
        address user,
        uint256 amount,
        bytes calldata quote,
        bytes calldata sgxSignature
    ) external {
        require(msg.sender == teeEnclave, "UR: only TEE Enclave");
        require(sgxVerifier.verifyQuote(quote, sgxSignature), "UR: invalid SGX attestation");
        require(amount > 0, "UR: amount must be >0");
        require(amount <= userBalance[user], "UR: exceeds balance");

        userBalance[user] = userBalance[user] > amount ? userBalance[user] - amount : 0;
        totalRecovered += amount;

        require(IERC20(drmToken).transfer(user, amount), "UR: transfer failed");

        emit EmergencyWithdrawal(user, amount, keccak256(quote));
        emit RecoveryExecuted(user, amount);
    }

    // ==================== View Functions ====================

    function getRequest(address user) external view returns (RecoveryRequest memory) {
        return requests[user];
    }

    function getNonce(address user) external view returns (uint256) {
        return nonces[user];
    }

    function getUserBalance(address user) external view returns (uint256) {
        return userBalance[user];
    }

    function getUserCount() external view returns (uint256) {
        return registeredUsers.length();
    }

    function getUserAt(uint256 index) external view returns (address) {
        return registeredUsers.at(index);
    }

    function _hashTypedDataV4(bytes32 innerHash) internal view returns (bytes32) {
        return keccak256(abi.encodePacked("\x19\x01", DOMAIN_SEPARATOR, innerHash));
    }
}
