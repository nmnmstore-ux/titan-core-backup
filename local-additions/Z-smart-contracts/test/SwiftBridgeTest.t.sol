// SPDX-License-Identifier: SWIFTBRIDGE
pragma solidity ^0.8.24;

import "forge-std/Test.sol";
import "../SwiftBridgeDAO.sol";
import "../DRS.sol";
import "../UnilateralRecovery.sol";
import "../RWA.sol";
import "../Token.sol";

contract SwiftBridgeTest is Test {
    SWBToken public token;
    SwiftBridgeDAO public dao;
    DynamicRebateSystem public drs;
    UnilateralRecovery public ur;
    RWABacking public rwa;

    address public founder = address(0x1);
    address public user1 = address(0x2);
    address public user2 = address(0x3);
    address public teeEnclave = address(0x999);

    function setUp() public {
        vm.startPrank(founder);

        // Deploy token
        token = new SWBToken();
        assertEq(token.totalSupply(), 10_000_000 * 10**18);

        // Deploy DAO
        dao = new SwiftBridgeDAO(address(token), teeEnclave);
        assertFalse(dao.isHalted());

        // Deploy DRS
        drs = new DynamicRebateSystem(address(token));
        assertEq(drs.REBATE_POOL_PCT(), 35);

        // Deploy Unilateral Recovery
        ur = new UnilateralRecovery(address(token), address(dao), teeEnclave);
        assertEq(ur.RECOVERY_TIMELOCK(), 30 days);
        assertEq(ur.MAX_RECOVERY_PCT(), 95);

        // Deploy RWA
        rwa = new RWABacking();
        assertTrue(rwa.isSolvent());

        // Fund users
        token.transfer(user1, 100_000 * 10**18);
        token.transfer(user2, 50_000 * 10**18);

        vm.stopPrank();
    }

    // ==================== Token Tests ====================
    function testTokenBasics() public {
        assertEq(token.name(), "SwiftBridge");
        assertEq(token.symbol(), "SWB");
        assertEq(token.decimals(), 18);
        assertEq(token.totalSupply(), 10_000_000 * 10**18);
    }

    function testTokenMaxSupply() public {
        vm.prank(founder);
        vm.expectRevert("SWB: max supply reached");
        token.mint(address(this), 100_000_000 * 10**18);
    }

    function testTokenStaking() public {
        vm.startPrank(user1);
        uint256 amount = 10_000 * 10**18;
        token.approve(address(drs), amount);
        drs.stake(amount);
        assertEq(drs.staked(user1), amount);
        vm.stopPrank();
    }

    function testTokenRebate() public {
        vm.startPrank(user1);
        uint256 stakeAmount = 50_000 * 10**18;
        token.approve(address(drs), stakeAmount);
        drs.stake(stakeAmount);
        assertEq(drs.getUserTier(user1), 2);
        vm.stopPrank();
    }

    // ==================== DAO Tests ====================
    function testDAOCreation() public {
        assertEq(address(dao.swbToken()), address(token));
        assertEq(dao.MIN_VOTING_POWER(), 1000 * 10**18);
        assertEq(dao.VOTING_PERIOD(), 7 days);
        assertEq(dao.QUORUM(), 4_000_000 * 10**18);
    }

    function testDAOPropose() public {
        vm.prank(founder);
        uint256 id = dao.propose(
            SwiftBridgeDAO.ProposalType.FeeChange,
            "Reduce fees by 50%",
            hex""
        );
        assertEq(id, 1);

        (,,, string memory desc,,,,,) = dao.getProposal(id);
        assertEq(desc, "Reduce fees by 50%");
    }

    function testDAOVote() public {
        vm.startPrank(founder);
        uint256 id = dao.propose(
            SwiftBridgeDAO.ProposalType.ParameterUpdate,
            "Test proposal",
            hex""
        );
        dao.castVote(id, true);
        vm.stopPrank();
    }

    function testDAOQuorumNotMet() public {
        vm.prank(founder);
        uint256 id = dao.propose(
            SwiftBridgeDAO.ProposalType.AgentConfig,
            "Change agent params",
            hex""
        );
        vm.warp(block.timestamp + 8 days);
        vm.prank(user1);
        vm.expectRevert("DAO: quorum not met");
        dao.execute(id);
    }

    function testDAOEmergencyHalt() public {
        vm.prank(teeEnclave);
        dao.emergencyHalt();
        assertTrue(dao.isHalted());
    }

    function testDAOEmergencyResume() public {
        vm.prank(teeEnclave);
        dao.emergencyHalt();
        dao.emergencyResume();
        assertFalse(dao.isHalted());
    }

    function testDAOCannotProposeWhenHalted() public {
        vm.prank(teeEnclave);
        dao.emergencyHalt();
        vm.prank(founder);
        vm.expectRevert("DAO: halted");
        dao.propose(SwiftBridgeDAO.ProposalType.FeeChange, "test", hex"");
    }

    // ==================== Unilateral Recovery Tests ====================
    function testRecoveryRequest() public {
        vm.prank(user1);
        bytes memory sig = _signRecovery(user1, 1000 * 10**18);
        ur.requestRecovery(1000 * 10**18, sig);
        (address u, uint256 amt,, bool executed,) = ur.getRequest(user1);
        assertEq(u, user1);
        assertEq(amt, 1000 * 10**18);
        assertFalse(executed);
    }

    function testRecoveryTimelock() public {
        vm.prank(user1);
        bytes memory sig = _signRecovery(user1, 1000 * 10**18);
        ur.requestRecovery(1000 * 10**18, sig);

        vm.expectRevert("UR: timelock not expired");
        ur.executeRecovery(user1);
    }

    function testRecoveryAfterTimelock() public {
        // Fund the contract with tokens
        vm.prank(founder);
        token.transfer(address(ur), 100_000 * 10**18);

        vm.prank(user1);
        bytes memory sig = _signRecovery(user1, 1000 * 10**18);
        ur.requestRecovery(1000 * 10**18, sig);

        vm.warp(block.timestamp + 31 days);

        ur.executeRecovery(user1);
        (,,, bool executed,) = ur.getRequest(user1);
        assertTrue(executed);
    }

    function testRecoveryMax95Percent() public {
        vm.prank(founder);
        token.transfer(address(ur), 100_000 * 10**18);

        vm.prank(user1);
        bytes memory sig = _signRecovery(user1, 100_000 * 10**18);
        ur.requestRecovery(100_000 * 10**18, sig);
        vm.warp(block.timestamp + 31 days);

        uint256 balanceBefore = token.balanceOf(user1);
        ur.executeRecovery(user1);
        uint256 recovered = token.balanceOf(user1) - balanceBefore;

        assertLe(recovered, 95_000 * 10**18);
    }

    function testTEEEmergencyWithdraw() public {
        vm.prank(founder);
        token.transfer(address(ur), 10_000 * 10**18);

        uint256 balanceBefore = token.balanceOf(user1);
        vm.prank(teeEnclave);
        ur.teeEmergencyWithdraw(user1, 5_000 * 10**18, hex"deadbeef");
        uint256 recovered = token.balanceOf(user1) - balanceBefore;
        assertEq(recovered, 5_000 * 10**18);
    }

    // ==================== DRS Tests ====================
    function testDRSVolumeRebate() public {
        vm.prank(founder);
        token.transfer(address(drs), 10_000 * 10**18);

        vm.prank(address(dao));
        drs.recordVolume(user1, 100_000 * 10**18, 1_000 * 10**18);
        uint256 rebate = drs.getRebate(user1);
        assertGt(rebate, 0);
        assertEq(rebate, (1_000 * 10**18 * 35) / 100);
    }

    function testDRSTierMultiplier() public {
        vm.startPrank(user2);
        uint256 stakeAmount = 250_000 * 10**18;
        token.approve(address(drs), stakeAmount);
        drs.stake(stakeAmount);
        assertEq(drs.getUserTier(user2), 3);
        assertEq(drs.getTierMultiplier(user2), 200);
        vm.stopPrank();
    }

    function testDRSClaimRebate() public {
        vm.prank(founder);
        token.transfer(address(drs), 50_000 * 10**18);

        vm.prank(address(dao));
        drs.recordVolume(user1, 500_000 * 10**18, 5_000 * 10**18);

        vm.prank(user1);
        drs.claimRebate();
        assertEq(drs.getRebate(user1), 0);
    }

    // ==================== RWA Tests ====================
    function testRWAAddReserve() public {
        vm.prank(founder);
        rwa.addReserve("GOLD", 1000, 65_000_000, address(0x4), keccak256("proof"));
        assertEq(rwa.getReserveCount(), 1);
    }

    function testRWASolvency() public {
        assertTrue(rwa.isSolvent());
        vm.prank(founder);
        rwa.addReserve("GOLD", 100, 6_500_000, address(0x4), keccak256("proof"));
        rwa.mintBacked(user1, 5_000_000 * 10**18);
        assertTrue(rwa.isSolvent());
    }

    function testRWACannotMintExcess() public {
        vm.prank(founder);
        vm.expectRevert("RWA: insufficient reserves");
        rwa.mintBacked(user1, 1_000_000_000 * 10**18);
    }

    function testRWAAudit() public {
        vm.prank(founder);
        rwa.completeAudit(keccak256("audit_data"), "Deloitte");
    }

    // ==================== Helper ====================
    function _signRecovery(address user, uint256 amount) internal view returns (bytes memory) {
        bytes32 message = keccak256(abi.encodePacked(user, amount, block.timestamp));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(1, message);
        return abi.encodePacked(r, s, v);
    }
}
