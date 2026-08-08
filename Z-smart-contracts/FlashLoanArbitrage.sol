// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

// ============================================================
// THE-Bridge Flash Loan Arbitrage Executor
//
// Executes Aave V3 flash-loan based cross-pool arbitrage:
// borrow an asset via Aave V3 `flashLoanSimple`, swap through a
// Uniswap UniversalRouter multi-hop `path` (V3_SWAP_EXACT_IN),
// repay principal + flash fee, and keep the remainder as profit.
//
// Security posture:
//   - Only the Aave V3 pool may trigger `executeOperation`.
//   - Only the owner may launch an arbitrage or change settings.
//   - minAmountOut / slipProtection guard every swap hop.
//   - Reentrancy guard on the flash entrypoint.
// ============================================================

interface IERC20 {
    function balanceOf(address account) external view returns (uint256);
    function approve(address spender, uint256 amount) external returns (bool);
    function transfer(address to, uint256 amount) external returns (bool);
}

interface IPool {
    // Aave V3: flashLoanSimple(receiver, asset, amount, params, referralCode)
    function flashLoanSimple(
        address receiver,
        address asset,
        uint256 amount,
        bytes calldata params,
        uint16 referralCode
    ) external;

    // Aave V3 callback (msg.sender == pool)
    function executeOperation(
        address asset,
        uint256 amount,
        uint256 premium,
        address initiator,
        bytes calldata params
    ) external returns (bool);
}

interface IUniversalRouter {
    // Uniswap UniversalRouter: execute(bytes commands, bytes[] inputs)
    function execute(bytes calldata commands, bytes[] calldata inputs) external payable;
}

// Aave V3 Pool, Sepolia.
address constant AAVE_V3_POOL_SEPOLIA = 0x6Ae43d3271ff6888e7Fc43Fd7321a503ff738951;
// Uniswap UniversalRouter, Sepolia (used by Uniswap UI).
address constant UNIVERSAL_ROUTER_SEPOLIA = 0x3fC91A3afd70395Cd496C647d5a6CC9D4B2b7FAD;

contract FlashLoanArbitrage {
    address public immutable pool;
    address public immutable swapRouter;
    address public owner;

    uint256 public flashFeeBps = 9; // calibration baseline; set post-deploy
    uint256 public slipProtectionBps = 50; // 0.5% enforced buffer per hop

    uint256 private _inFlash;

    error Unauthorized();
    error FlashFailed();
    error SwapFailed();
    error InsufficientProfit(uint256 profit, uint256 minRequired);
    error InvalidPath();

    event ArbExecuted(
        address indexed asset,
        uint256 borrowed,
        uint256 repaid,
        uint256 profit
    );

    modifier onlyOwner() {
        if (msg.sender != owner) revert Unauthorized();
        _;
    }

    modifier onlyPool() {
        if (msg.sender != pool) revert Unauthorized();
        _;
    }

    constructor(address _pool, address _router) {
        pool = _pool;
        swapRouter = _router;
        owner = msg.sender;
    }

    receive() external payable {}

    function setOwner(address _o) external onlyOwner {
        owner = _o;
    }

    function setFlashFeeBps(uint256 _b) external onlyOwner {
        flashFeeBps = _b;
    }

    function setSlipProtectionBps(uint256 _b) external onlyOwner {
        slipProtectionBps = _b;
    }

    /// Withdraw accidental tokens sent to this contract.
    function rescue(address token, address to, uint256 amount) external onlyOwner {
        if (token == address(0)) {
            (bool ok, ) = to.call{value: amount}("");
            require(ok, "ETH transfer failed");
        } else {
            require(IERC20(token).transfer(to, amount), "rescue transfer failed");
        }
    }

    /// Entry point: flash-borrow `asset` and run the swap encoded in `params`.
    ///
    /// @param params ABI-encoded:
    ///   address[] path, uint24[] fees, uint256 minOutAfterFees, address recipient
    function executeArbitrage(
        address asset,
        uint256 amount,
        bytes calldata params
    ) external onlyOwner {
        if (_inFlash != 0) revert FlashFailed();
        _inFlash = 1;

        IPool(pool).flashLoanSimple(address(this), asset, amount, params, 0);

        _inFlash = 0;
    }

    /// Aave V3 callback — swap, repay, and verify profit.
    function executeOperation(
        address asset,
        uint256 amount,
        uint256 premium,
        address initiator,
        bytes calldata params
    ) external onlyPool returns (bool) {
        if (_inFlash != 1 || initiator != address(this)) revert FlashFailed();

        (address[] memory path, uint24[] memory fees, uint256 minOutAfterFees, address recipient) =
            abi.decode(params, (address[], uint24[], uint256, address));

        if (path.length < 2) revert InvalidPath();
        if (path[0] != asset) revert InvalidPath();

        uint256 balanceBefore = IERC20(asset).balanceOf(address(this));

        // Seed the UniversalRouter with the flash-borrowed input tokens.
        require(IERC20(asset).approve(swapRouter, amount), "approve router failed");
        require(IERC20(asset).transfer(swapRouter, amount), "seed router failed");

        uint256 out = _swapPath(path, fees, amount, balanceBefore);

        uint256 repay = amount + premium;
        uint256 profit = out > repay ? out - repay : 0;

        if (profit < minOutAfterFees) {
            revert InsufficientProfit(profit, minOutAfterFees);
        }

        IERC20(asset).approve(pool, repay);
        require(IERC20(asset).transfer(pool, repay), "repay transfer failed");

        if (recipient != address(0) && profit > 0) {
            require(IERC20(asset).transfer(recipient, profit), "profit transfer failed");
        }

        emit ArbExecuted(asset, amount, repay, profit);
        return true;
    }

    /// Swap through a sequence of Uniswap V3 pools via the UniversalRouter
    /// (command 0x00 = V3_SWAP_EXACT_IN, payerIsUser = false).
    function _swapPath(
        address[] memory path,
        uint24[] memory fees,
        uint256 amountIn,
        uint256 balanceBefore
    ) internal returns (uint256 amountOut) {
        // Build the concatenated V3 path: tokenIn, fee, tokenMid, fee, ..., tokenOut
        bytes memory v3Path = abi.encodePacked(path[0], fees[0]);
        for (uint256 i = 1; i < path.length - 1; i++) {
            v3Path = abi.encodePacked(v3Path, path[i], fees[i]);
        }
        v3Path = abi.encodePacked(v3Path, path[path.length - 1]);

        uint256 minOut = (amountIn * (10000 - slipProtectionBps)) / 10000;

        bytes memory commands = abi.encodePacked(bytes1(uint8(0x00))); // V3_SWAP_EXACT_IN

        bytes[] memory inputs = new bytes[](1);
        inputs[0] = abi.encode(
            address(this), // recipient of swap output
            amountIn,
            minOut,
            v3Path,
            false // payerIsUser = false: funds already in the router
        );

        IUniversalRouter(swapRouter).execute(commands, inputs);

        amountOut = IERC20(path[path.length - 1]).balanceOf(address(this));
        if (amountOut <= balanceBefore) revert SwapFailed();
    }
}
