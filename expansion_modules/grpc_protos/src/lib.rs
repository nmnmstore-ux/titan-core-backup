pub mod thebridge {
    tonic::include_proto!("thebridge");
}

pub use thebridge::{
    ai_ceo_service_client, ai_ceo_service_server, auth_service_client, auth_service_server,
    fiat_service_client, fiat_service_server, health_client, health_server, ErrorDetail,
    LiquidityRequest, AnalysisResponse, LiquidityVerdict, SlippageRequest, SlippageAlert,
    AlertSeverity, OrderSide, TradingModeResponse, TradingMode, MarketTick, Signal, SignalKind,
    RegistrationRequest, RegistrationResponse, LoginRequest, LoginResponse, TokenRequest,
    RefreshRequest, SessionInfo as GrpcSessionInfo, DepositRequest, DepositResponse,
    DepositMethod, WithdrawalRequest, WithdrawalResponse, WithdrawalMethod, CardRequest,
    CardResponse, CardIdRequest, CardStatus, TxStatus, HistoryRequest, TransactionRecord,
    TransactionHistoryResponse, HealthCheckRequest, HealthCheckResponse, Empty, Timestamp, Money,
    WalletAddress, TransactionId as GrpcTransactionId, RequestMeta, ErrorCode,
};
