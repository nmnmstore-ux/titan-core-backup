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
    MockPriceOracle public oracle;

    address public founder = address(0x1);
    address public user1 = address(0x2);
    address public user2 = address(0x3);
    address public teeEnclave = address(0x999);
    address public sgxVerifier = address(0x888);
    uint256 public founderKey = 1;

    function setUp() public {
        vm.startPrank(founder);

        token = new SWBToken();
        assertEq(token.totalSupply(), 10_000_000 * 10**18);

        dao = new SwiftBridgeDAO(address(token), teeEnclave);
        assertFalse(dao.isHalted());

        drs = new DynamicRebateSystem(address(token));
        assertEq(drs.REBATE_POOL_PCT(), 35);

        ur = new UnilateralRecovery(address(token), address(dao), sgxVerifier, teeEnclave);
        assertEq(ur.RECOVERY_TIMELOCK(), 30 days);

        rwa = new RWABacking();
        assertTrue(rwa.isSolvent());

        oracle = new MockPriceOracle();

        token.transfer(user1, 100_000 * 10**18);
        token.transfer(user2, 50_000 * 10**18);

        vm.stopPrank();
    }

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

    function testRecoveryRequest() public {
        vm.prank(founder);
        token.transfer(address(ur), 100_000 * 10**18);
        ur.setBalance(user1, 100_000 * 10**18);

        vm.prank(user1);
        bytes memory sig = _signEIP712Recovery(user1, 1000 * 10**18, 0, block.timestamp + 1 hours);
        ur.requestRecovery(1000 * 10**18, block.timestamp + 1 hours, sig);
        (address u, uint256 amt,,, bool executed,) = ur.getRequest(user1);
        assertEq(u, user1);
        assertEq(amt, 1000 * 10**18);
        assertFalse(executed);
    }

    function testRecoveryTimelock() public {
        vm.prank(founder);
        token.transfer(address(ur), 100_000 * 10**18);
        ur.setBalance(user1, 100_000 * 10**18);

        vm.prank(user1);
        bytes memory sig = _signEIP712Recovery(user1, 1000 * 10**18, 0, block.timestamp + 1 hours);
        ur.requestRecovery(1000 * 10**18, block.timestamp + 1 hours, sig);

        vm.expectRevert("UR: timelock not expired");
        ur.executeRecovery(user1);
    }

    function testRecoveryAfterTimelock() public {
        vm.prank(founder);
        token.transfer(address(ur), 100_000 * 10**18);
        ur.setBalance(user1, 100_000 * 10**18);

        vm.prank(user1);
        bytes memory sig = _signEIP712Recovery(user1, 1000 * 10**18, 0, block.timestamp + 1 hours);
        ur.requestRecovery(1000 * 10**18, block.timestamp + 1 hours, sig);

        vm.warp(block.timestamp + 31 days);

        ur.executeRecovery(user1);
        (,,, bool executed,) = ur.getRequest(user1);
        assertTrue(executed);
    }

    function testTEEEmergencyWithdraw() public {
        vm.prank(founder);
        token.transfer(address(ur), 10_000 * 10**18);
        ur.setBalance(user1, 10_000 * 10**18);

        uint256 balanceBefore = token.balanceOf(user1);
        vm.prank(teeEnclave);
        ur.teeEmergencyWithdraw(user1, 5_000 * 10**18, hex"deadbeef", hex"");
        uint256 recovered = token.balanceOf(user1) - balanceBefore;
        assertEq(recovered, 5_000 * 10**18);
    }

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

    function _signEIP712Recovery(
        address user,
        uint256 amount,
        uint256 nonce,
        uint256 deadline
    ) internal view returns (bytes memory) {
        bytes32 DOMAIN_SEPARATOR = keccak256(
            abi.encode(
                keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"),
                keccak256("SwiftBridge Unilateral Recovery"),
                keccak256("2"),
                block.chainid,
                address(ur)
            )
        );
        bytes32 RECOVERY_TYPEHASH = keccak256(
            "RecoveryRequest(address user,uint256 amount,uint256 nonce,uint256 deadline)"
        );
        bytes32 structHash = keccak256(abi.encode(
            RECOVERY_TYPEHASH, user, amount, nonce, deadline
        ));
        bytes32 digest = keccak256(abi.encodePacked("\x19\x01", DOMAIN_SEPARATOR, structHash));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(founderKey, digest);
        return abi.encodePacked(r, s, v);
    }
}

contract MockPriceOracle {
    mapping(string => uint256) public prices;

    constructor() {
        prices["USD/USB"] = 100 * (10**18);
        prices["USB/USD"] = 1 * (10**18) / 100;
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