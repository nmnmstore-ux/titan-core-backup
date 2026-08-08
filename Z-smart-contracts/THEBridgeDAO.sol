// ============================================================
// THE-Bridge DAO - Decentralized Autonomous Organization
// Sovereign Master Prompt: الكود هو القانون الوحيد
// لا موظفين، لا مجلس إدارة، لا كيان بشري
// ============================================================

// SPDX-License-Identifier: THE-BRIDGE
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import "@openzeppelin/contracts/utils/cryptography/MerkleProof.sol";

contract THEBridgeDAO {
    using ECDSA for bytes32;

    // ==================== State ====================
    address public immutable drmToken;
    uint256 public constant MIN_VOTING_POWER = 1000 * 10**18; // 1000 DRM to vote
    uint256 public constant VOTING_PERIOD = 7 days;
    uint256 public constant EXECUTION_DELAY = 2 days;
    uint256 public constant QUORUM = 4_000_000 * 10**18; // 4M DRM quorum

    enum ProposalState { Pending, Active, Succeeded, Executed, Defeated, Canceled }
    enum ProposalType { FeeChange, AgentConfig, FundAllocation, ParameterUpdate, EmergencyHalt }

    struct Proposal {
        uint256 id;
        address proposer;
        ProposalType pType;
        string description;
        bytes calldataData;
        uint256 forVotes;
        uint256 againstVotes;
        uint256 createdAt;
        uint256 deadline;
        ProposalState state;
        bool executed;
    }

    mapping(uint256 => Proposal) public proposals;
    mapping(uint256 => mapping(address => bool)) public hasVoted;
    uint256 public proposalCount;
    bool public halted;
    address public immutable teeEnclave; // TEE address for automated operations

    // ==================== Events ====================
    event ProposalCreated(uint256 indexed id, address proposer, ProposalType pType, string description);
    event VoteCast(uint256 indexed proposalId, address voter, bool support, uint256 weight);
    event ProposalExecuted(uint256 indexed id);
    event EmergencyHalt(bool halted);

    constructor(address _drmToken, address _teeEnclave) {
        drmToken = _drmToken;
        teeEnclave = _teeEnclave;
        halted = false;
    }

    // ==================== Proposal Lifecycle ====================
    function propose(ProposalType pType, string calldata description, bytes calldata data) external returns (uint256) {
        require(!halted, "DAO: halted");
        require(ERC20(drmToken).balanceOf(msg.sender) >= MIN_VOTING_POWER, "DAO: insufficient voting power");

        proposalCount++;
        Proposal storage p = proposals[proposalCount];
        p.id = proposalCount;
        p.proposer = msg.sender;
        p.pType = pType;
        p.description = description;
        p.calldataData = data;
        p.createdAt = block.timestamp;
        p.deadline = block.timestamp + VOTING_PERIOD;
        p.state = ProposalState.Active;

        emit ProposalCreated(proposalCount, msg.sender, pType, description);
        return proposalCount;
    }

    function castVote(uint256 proposalId, bool support) external {
        Proposal storage p = proposals[proposalId];
        require(p.state == ProposalState.Active, "DAO: not active");
        require(block.timestamp <= p.deadline, "DAO: voting ended");
        require(!hasVoted[proposalId][msg.sender], "DAO: already voted");

        uint256 weight = ERC20(drmToken).balanceOf(msg.sender);
        require(weight >= MIN_VOTING_POWER, "DAO: insufficient voting power");

        hasVoted[proposalId][msg.sender] = true;
        if (support) {
            p.forVotes += weight;
        } else {
            p.againstVotes += weight;
        }

        emit VoteCast(proposalId, msg.sender, support, weight);
    }

    function execute(uint256 proposalId) external {
        Proposal storage p = proposals[proposalId];
        require(p.state == ProposalState.Active, "DAO: not active");
        require(block.timestamp > p.deadline, "DAO: voting not ended");
        require(block.timestamp <= p.deadline + EXECUTION_DELAY, "DAO: execution window passed");
        require(!p.executed, "DAO: already executed");
        require(p.forVotes > p.againstVotes, "DAO: not enough for votes");
        require(p.forVotes + p.againstVotes >= QUORUM, "DAO: quorum not met");

        p.state = ProposalState.Executed;
        p.executed = true;

        // Execute the proposal
        (bool success, ) = address(this).call(p.calldataData);
        require(success, "DAO: execution failed");

        emit ProposalExecuted(proposalId);
    }

    // ==================== Emergency Controls ====================
    function emergencyHalt() external {
        require(
            msg.sender == teeEnclave || ERC20(drmToken).balanceOf(msg.sender) >= 10_000_000 * 10**18,
            "DAO: unauthorized"
        );
        halted = true;
        emit EmergencyHalt(true);
    }

    function emergencyResume() external {
        require(
            msg.sender == teeEnclave || ERC20(drmToken).balanceOf(msg.sender) >= 10_000_000 * 10**18,
            "DAO: unauthorized"
        );
        halted = false;
        emit EmergencyHalt(false);
    }

    // ==================== View ====================
    function getProposal(uint256 id) external view returns (Proposal memory) {
        return proposals[id];
    }

    function isHalted() external view returns (bool) {
        return halted;
    }
}
