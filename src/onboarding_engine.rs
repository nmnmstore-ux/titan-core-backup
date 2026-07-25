use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingConfig {
    pub auto_kyc_enabled: bool,
    pub auto_aml_enabled: bool,
    pub document_verification_ai: bool,
    pub sanctions_screening_realtime: bool,
    pub pep_screening_realtime: bool,
    pub adverse_media_monitoring: bool,
    pub enhanced_due_diligence_threshold_usd: u64,
    pub onboarding_sla_hours: u32,
    pub prime_broker_integrations: Vec<PrimeBrokerConfig>,
    pub custodian_integrations: Vec<CustodianConfig>,
    pub settlement_instructions: SettlementConfig,
    pub regulatory_reporting_auto: bool,
    pub audit_trail_immutable: bool,
}

impl Default for OnboardingConfig {
    fn default() -> Self {
        Self {
            auto_kyc_enabled: true,
            auto_aml_enabled: true,
            document_verification_ai: true,
            sanctions_screening_realtime: true,
            pep_screening_realtime: true,
            adverse_media_monitoring: true,
            enhanced_due_diligence_threshold_usd: 1_000_000,
            onboarding_sla_hours: 24,
            prime_broker_integrations: vec![
                PrimeBrokerConfig {
                    broker_id: "GOLDMAN_SACHS".to_string(),
                    name: "Goldman Sachs Prime Brokerage".to_string(),
                    api_endpoint: "https://api.gs.com/prime".to_string(),
                    supported_assets: vec!["Equities".to_string(), "Fixed Income".to_string(), "FX".to_string(), "Derivatives".to_string()],
                    margin_rates: HashMap::from([
                        ("Equities".to_string(), 0.02),
                        ("Fixed Income".to_string(), 0.01),
                        ("FX".to_string(), 0.005),
                        ("Derivatives".to_string(), 0.03),
                    ]),
                    settlement_cycle: "T+2".to_string(),
                    custody_fee_bps: 2,
                    enabled: true,
                },
                PrimeBrokerConfig {
                    broker_id: "MORGAN_STANLEY".to_string(),
                    name: "Morgan Stanley Prime Brokerage".to_string(),
                    api_endpoint: "https://api.ms.com/prime".to_string(),
                    supported_assets: vec!["Equities".to_string(), "Fixed Income".to_string(), "FX".to_string(), "Commodities".to_string()],
                    margin_rates: HashMap::from([
                        ("Equities".to_string(), 0.025),
                        ("Fixed Income".to_string(), 0.015),
                        ("FX".to_string(), 0.008),
                        ("Commodities".to_string(), 0.04),
                    ]),
                    settlement_cycle: "T+2".to_string(),
                    custody_fee_bps: 3,
                    enabled: true,
                },
                PrimeBrokerConfig {
                    broker_id: "JPMORGAN".to_string(),
                    name: "J.P. Morgan Prime Brokerage".to_string(),
                    api_endpoint: "https://api.jpmorgan.com/prime".to_string(),
                    supported_assets: vec!["Equities".to_string(), "Fixed Income".to_string(), "FX".to_string(), "Derivatives".to_string(), "Crypto".to_string()],
                    margin_rates: HashMap::from([
                        ("Equities".to_string(), 0.018),
                        ("Fixed Income".to_string(), 0.01),
                        ("FX".to_string(), 0.004),
                        ("Derivatives".to_string(), 0.025),
                        ("Crypto".to_string(), 0.05),
                    ]),
                    settlement_cycle: "T+1".to_string(),
                    custody_fee_bps: 1,
                    enabled: true,
                },
            ],
            custodian_integrations: vec![
                CustodianConfig {
                    custodian_id: "BNY_MELLON".to_string(),
                    name: "BNY Mellon".to_string(),
                    api_endpoint: "https://api.bnymellon.com/custody".to_string(),
                    supported_markets: vec!["US".to_string(), "EU".to_string(), "UK".to_string(), "APAC".to_string()],
                    settlement_systems: vec!["DTCC".to_string(), "Euroclear".to_string(), "Clearstream".to_string()],
                    fee_bps: 1,
                    enabled: true,
                },
                CustodianConfig {
                    custodian_id: "STATE_STREET".to_string(),
                    name: "State Street".to_string(),
                    api_endpoint: "https://api.statestreet.com/custody".to_string(),
                    supported_markets: vec!["US".to_string(), "EU".to_string(), "UK".to_string()],
                    settlement_systems: vec!["DTCC".to_string(), "Euroclear".to_string()],
                    fee_bps: 2,
                    enabled: true,
                },
            ],
            settlement_instructions: SettlementConfig {
                default_settlement_cycle: "T+1".to_string(),
                supported_currencies: vec!["USD".to_string(), "EUR".to_string(), "GBP".to_string(), "JPY".to_string(), "AED".to_string()],
                dvp_enabled: true,
                pvp_enabled: true,
                netting_enabled: true,
                fail_management_auto: true,
            },
            regulatory_reporting_auto: true,
            audit_trail_immutable: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimeBrokerConfig {
    pub broker_id: String,
    pub name: String,
    pub api_endpoint: String,
    pub supported_assets: Vec<String>,
    pub margin_rates: HashMap<String, f64>,
    pub settlement_cycle: String,
    pub custody_fee_bps: u32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustodianConfig {
    pub custodian_id: String,
    pub name: String,
    pub api_endpoint: String,
    pub supported_markets: Vec<String>,
    pub settlement_systems: Vec<String>,
    pub fee_bps: u32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementConfig {
    pub default_settlement_cycle: String,
    pub supported_currencies: Vec<String>,
    pub dvp_enabled: bool,
    pub pvp_enabled: bool,
    pub netting_enabled: bool,
    pub fail_management_auto: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OnboardingStatus {
    Initiated,
    DocumentCollection,
    DocumentVerification,
    KYCReview,
    AMLScreening,
    SanctionsScreening,
    PEPScreening,
    AdverseMediaCheck,
    EnhancedDueDiligence,
    RiskAssessment,
    AccountSetup,
    PrimeBrokerLinking,
    CustodianSetup,
    SettlementInstructionSetup,
    ComplianceOfficerApproval,
    LegalReview,
    Activated,
    Rejected,
    OnHold,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EntityType {
    Individual,
    Corporation,
    Partnership,
    Trust,
    Fund,
    FamilyOffice,
    SovereignWealthFund,
    CentralBank,
    GovernmentEntity,
    NonProfit,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstitutionalClient {
    pub client_id: String,
    pub legal_name: String,
    pub entity_type: EntityType,
    pub jurisdiction: String,
    pub lei: Option<String>,
    pub tax_id: Option<String>,
    pub registration_number: Option<String>,
    pub registered_address: Address,
    pub operational_address: Address,
    pub website: Option<String>,
    pub primary_contact: Contact,
    pub authorized_signatories: Vec<AuthorizedSignatory>,
    pub beneficial_owners: Vec<BeneficialOwner>,
    pub directors: Vec<Director>,
    pub shareholders: Vec<Shareholder>,
    pub regulatory_licenses: Vec<RegulatoryLicense>,
    pub banking_relationships: Vec<BankingRelationship>,
    pub prime_broker_preference: Option<String>,
    pub custodian_preference: Option<String>,
    pub settlement_instructions: ClientSettlementInstructions,
    pub investment_objectives: InvestmentObjectives,
    pub risk_tolerance: RiskTolerance,
    pub expected_monthly_volume_usd: u64,
    pub expected_aum_usd: u64,
    pub source_of_funds: SourceOfFunds,
    pub aml_risk_rating: AMLRiskRating,
    pub status: OnboardingStatus,
    pub onboarding_started_at: u64,
    pub onboarding_completed_at: Option<u64>,
    pub kyc_completed_at: Option<u64>,
    pub kyc_expires_at: Option<u64>,
    pub compliance_officer: Option<String>,
    pub legal_counsel: Option<String>,
    pub documents: Vec<ClientDocument>,
    pub audit_trail: Vec<AuditEntry>,
    pub tags: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address {
    pub line1: String,
    pub line2: Option<String>,
    pub city: String,
    pub state: Option<String>,
    pub postal_code: String,
    pub country: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub name: String,
    pub title: String,
    pub email: String,
    pub phone: String,
    pub preferred_contact_method: ContactMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContactMethod {
    Email,
    Phone,
    SecureMessage,
    InPerson,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizedSignatory {
    pub signatory_id: String,
    pub name: String,
    pub title: String,
    pub email: String,
    pub signing_authority: SigningAuthority,
    pub specimen_signature: Option<String>,
    pub id_document: Option<String>,
    pub verified: bool,
    pub verified_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SigningAuthority {
    Sole,
    JointTwo,
    JointAny,
    Limited(String),
    TradingOnly,
    SettlementOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeneficialOwner {
    pub owner_id: String,
    pub name: String,
    pub ownership_pct: f64,
    pub nationality: String,
    pub country_of_residence: String,
    pub date_of_birth: u64,
    pub id_number: String,
    pub id_type: String,
    pub pep_status: bool,
    pub sanctions_check: bool,
    pub adverse_media: bool,
    pub kyc_status: KYCStatus,
    pub source_of_wealth: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum KYCStatus {
    NotStarted,
    InProgress,
    PendingReview,
    Approved,
    Rejected,
    Expired,
    RequiresEnhancedDueDiligence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Director {
    pub director_id: String,
    pub name: String,
    pub nationality: String,
    pub country_of_residence: String,
    pub date_of_birth: u64,
    pub appointment_date: u64,
    pub is_executive: bool,
    pub committees: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shareholder {
    pub shareholder_id: String,
    pub name: String,
    pub ownership_pct: f64,
    pub share_class: String,
    pub voting_rights: bool,
    pub beneficial_owner_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegulatoryLicense {
    pub license_id: String,
    pub regulator: String,
    pub license_type: String,
    pub license_number: String,
    pub issued_at: u64,
    pub expires_at: Option<u64>,
    pub jurisdiction: String,
    pub status: String,
    pub activities_permitted: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankingRelationship {
    pub bank_name: String,
    pub account_number: String,
    pub account_type: String,
    pub currency: String,
    pub relationship_since: u64,
    pub reference_contact: String,
    pub swift_bic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientSettlementInstructions {
    pub default_custodian: String,
    pub settlement_cycle: String,
    pub dvp_enabled: bool,
    pub pvp_enabled: bool,
    pub netting_preference: NettingPreference,
    pub fail_tolerance_days: u32,
    pub cash_sweep_enabled: bool,
    pub collateral_management: bool,
    pub tax_reclaim_service: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NettingPreference {
    Gross,
    NetByCurrency,
    NetByCounterparty,
    FullNetting,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvestmentObjectives {
    pub primary_objective: InvestmentObjective,
    pub secondary_objectives: Vec<InvestmentObjective>,
    pub return_target_pct: f64,
    pub risk_budget_pct: f64,
    pub liquidity_requirements: LiquidityRequirements,
    pub esg_requirements: Option<ESGRequirements>,
    pub restricted_investments: Vec<String>,
    pub mandate_type: MandateType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum InvestmentObjective {
    CapitalPreservation,
    IncomeGeneration,
    CapitalGrowth,
    TotalReturn,
    Hedging,
    Arbitrage,
    MarketMaking,
    LiquidityProvision,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityRequirements {
    pub daily_liquidity_pct: f64,
    pub weekly_liquidity_pct: f64,
    pub monthly_liquidity_pct: f64,
    pub notice_period_days: u32,
    pub redemption_frequency: RedemptionFrequency,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RedemptionFrequency {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    SemiAnnual,
    Annual,
    Locked(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ESGRequirements {
    pub esg_mandate: bool,
    pub excluded_sectors: Vec<String>,
    pub excluded_countries: Vec<String>,
    pub minimum_esg_score: f64,
    pub impact_investing_pct: f64,
    pub carbon_footprint_limit: Option<f64>,
    pub unpri_signatory: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MandateType {
    Discretionary,
    Advisory,
    ExecutionOnly,
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskTolerance {
    pub risk_profile: RiskProfile,
    pub max_drawdown_pct: f64,
    pub max_leverage: f64,
    pub var_limit_pct: f64,
    pub concentration_limits: HashMap<String, f64>,
    pub sector_limits: HashMap<String, f64>,
    pub currency_limits: HashMap<String, f64>,
    pub counterparty_limits: HashMap<String, f64>,
    pub stress_test_requirements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RiskProfile {
    Conservative,
    Moderate,
    Balanced,
    Growth,
    Aggressive,
    Speculative,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceOfFunds {
    pub primary_source: FundSource,
    pub secondary_sources: Vec<FundSource>,
    pub expected_inflows_usd_monthly: u64,
    pub expected_outflows_usd_monthly: u64,
    pub fund_flow_description: String,
    pub supporting_documents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FundSource {
    OperatingRevenue,
    InvestmentReturns,
    CapitalContributions,
    DonationsGrants,
    GovernmentFunding,
    AssetSales,
    Borrowings,
    CryptoProceeds,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AMLRiskRating {
    Low,
    Medium,
    High,
    VeryHigh,
    Prohibited,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientDocument {
    pub document_id: String,
    pub document_type: DocumentType,
    pub file_name: String,
    pub file_hash: String,
    pub file_size_bytes: u64,
    pub mime_type: String,
    pub uploaded_at: u64,
    pub uploaded_by: String,
    pub verified_at: Option<u64>,
    pub verified_by: Option<String>,
    pub ai_verified: bool,
    pub ai_confidence: Option<f64>,
    pub expiry_date: Option<u64>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DocumentType {
    CertificateOfIncorporation,
    ArticlesOfAssociation,
    MemorandumOfAssociation,
    RegisterOfDirectors,
    RegisterOfShareholders,
    RegisterOfBeneficialOwners,
    BusinessLicense,
    RegulatoryLicense,
    TaxRegistration,
    ProofOfAddress,
    BoardResolution,
    AuthorizedSignatoryList,
    SpecimenSignatures,
    AuditedFinancialStatements,
    ManagementAccounts,
    BankReferenceLetter,
    LegalOpinion,
    AMLPolicy,
    ComplianceManual,
    OrganizationalChart,
    OwnershipStructureChart,
    FundProspectus,
    PartnershipAgreement,
    TrustDeed,
    InsuranceCertificate,
    CybersecurityPolicy,
    BusinessContinuityPlan,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub entry_id: String,
    pub timestamp: u64,
    pub actor: String,
    pub action: String,
    pub stage: OnboardingStatus,
    pub details: HashMap<String, String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimeBrokerAccount {
    pub account_id: String,
    pub client_id: String,
    pub prime_broker_id: String,
    pub account_type: String,
    pub margin_agreement_signed: bool,
    pub margin_agreement_date: Option<u64>,
    pub credit_limit_usd: f64,
    pub used_margin_usd: f64,
    pub available_margin_usd: f64,
    pub margin_rate: f64,
    pub custody_fee_bps: u32,
    pub settlement_cycle: String,
    pub supported_products: Vec<String>,
    pub reporting_frequency: ReportingFrequency,
    pub status: AccountStatus,
    pub opened_at: u64,
    pub last_reviewed_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReportingFrequency {
    RealTime,
    Daily,
    Weekly,
    Monthly,
    Quarterly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AccountStatus {
    Pending,
    Active,
    Suspended,
    Closed,
    UnderReview,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustodianAccount {
    pub account_id: String,
    pub client_id: String,
    pub custodian_id: String,
    pub account_name: String,
    pub base_currency: String,
    pub supported_currencies: Vec<String>,
    pub settlement_systems: Vec<String>,
    pub fee_schedule: FeeSchedule,
    pub tax_services: TaxServices,
    pub corporate_actions: CorporateActionServices,
    pub proxy_voting: bool,
    pub securities_lending: SecuritiesLendingConfig,
    pub status: AccountStatus,
    pub opened_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeSchedule {
    pub custody_fee_bps: u32,
    pub transaction_fee_usd: f64,
    pub fx_fee_bps: u32,
    pub corporate_action_fee_usd: f64,
    pub proxy_voting_fee_usd: f64,
    pub minimum_fee_usd_monthly: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxServices {
    pub withholding_tax_reclaim: bool,
    pub tax_reporting: bool,
    pub certificate_management: bool,
    pub treaty_benefits: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorporateActionServices {
    pub mandatory_actions: bool,
    pub voluntary_actions: bool,
    pub voting_rights: bool,
    pub dividend_processing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecuritiesLendingConfig {
    pub enabled: bool,
    pub revenue_split_pct: f64,
    pub collateral_requirements: CollateralRequirements,
    pub borrower_restrictions: Vec<String>,
    pub minimum_loan_size_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollateralRequirements {
    pub acceptable_collateral: Vec<String>,
    pub haircut_schedule: HashMap<String, f64>,
    pub concentration_limits: HashMap<String, f64>,
    pub currency_matching_required: bool,
}

pub struct OnboardingEngine {
    config: OnboardingConfig,
    clients: Arc<RwLock<HashMap<String, InstitutionalClient>>>,
    prime_broker_accounts: Arc<RwLock<HashMap<String, PrimeBrokerAccount>>>,
    custodian_accounts: Arc<RwLock<HashMap<String, CustodianAccount>>>,
    document_queue: Arc<RwLock<Vec<DocumentVerificationTask>>>,
    workflow_engine: Arc<RwLock<WorkflowEngine>>,
    metrics: Arc<RwLock<OnboardingMetrics>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentVerificationTask {
    pub task_id: String,
    pub client_id: String,
    pub document_id: String,
    pub document_type: DocumentType,
    pub assigned_to: Option<String>,
    pub status: VerificationStatus,
    pub priority: u32,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub ai_result: Option<AIVerificationResult>,
    pub human_result: Option<HumanVerificationResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum VerificationStatus {
    Queued,
    AIProcessing,
    AICompleted,
    HumanReview,
    Approved,
    Rejected,
    RequiresResubmission,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIVerificationResult {
    pub verified: bool,
    pub confidence: f64,
    pub extracted_data: HashMap<String, String>,
    pub anomalies_detected: Vec<String>,
    pub model_version: String,
    pub processed_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanVerificationResult {
    pub verified: bool,
    pub reviewer: String,
    pub notes: String,
    pub reviewed_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkflowEngine {
    pub active_workflows: HashMap<String, WorkflowInstance>,
    pub completed_workflows: Vec<WorkflowInstance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInstance {
    pub workflow_id: String,
    pub client_id: String,
    pub current_stage: OnboardingStatus,
    pub stage_started_at: u64,
    pub stage_sla_hours: u32,
    pub completed_stages: Vec<CompletedStage>,
    pub pending_tasks: Vec<String>,
    pub assigned_officers: Vec<String>,
    pub escalated: bool,
    pub escalation_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedStage {
    pub stage: OnboardingStatus,
    pub started_at: u64,
    pub completed_at: u64,
    pub officer: String,
    pub outcome: StageOutcome,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StageOutcome {
    Approved,
    Rejected,
    Conditional,
    Escalated,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OnboardingMetrics {
    pub total_clients: u32,
    pub active_onboardings: u32,
    pub completed_this_month: u32,
    pub rejected_this_month: u32,
    pub avg_onboarding_time_hours: f64,
    pub sla_breaches: u32,
    pub documents_pending: u32,
    pub prime_broker_accounts: u32,
    pub custodian_accounts: u32,
    pub total_aum_usd: u64,
    pub total_expected_volume_usd: u64,
}

impl OnboardingEngine {
    pub fn new(config: OnboardingConfig) -> Self {
        Self {
            config,
            clients: Arc::new(RwLock::new(HashMap::new())),
            prime_broker_accounts: Arc::new(RwLock::new(HashMap::new())),
            custodian_accounts: Arc::new(RwLock::new(HashMap::new())),
            document_queue: Arc::new(RwLock::new(Vec::new())),
            workflow_engine: Arc::new(RwLock::new(WorkflowEngine::default())),
            metrics: Arc::new(RwLock::new(OnboardingMetrics::default())),
        }
    }

    pub async fn start(&self) -> Result<(), String> {
        self.start_workflow_processor().await;
        self.start_document_processor().await;
        self.start_sla_monitor().await;
        info!("Institutional onboarding engine started");
        Ok(())
    }

    pub async fn initiate_onboarding(
        &self,
        legal_name: String,
        entity_type: EntityType,
        jurisdiction: String,
        primary_contact: Contact,
        expected_monthly_volume_usd: u64,
        expected_aum_usd: u64,
    ) -> Result<InstitutionalClient, String> {
        if self.config.prime_broker_integrations.iter().all(|p| !p.enabled) {
            return Err("No prime broker integrations available".to_string());
        }

        let client_id = format!("CLIENT_{}", Uuid::new_v4().to_string()[..12].to_uppercase());
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        
        let client = InstitutionalClient {
            client_id: client_id.clone(),
            legal_name,
            entity_type,
            jurisdiction: jurisdiction.clone(),
            lei: None,
            tax_id: None,
            registration_number: None,
            registered_address: Address {
                line1: "".to_string(),
                line2: None,
                city: "".to_string(),
                state: None,
                postal_code: "".to_string(),
                country: jurisdiction,
            },
            operational_address: Address {
                line1: "".to_string(),
                line2: None,
                city: "".to_string(),
                state: None,
                postal_code: "".to_string(),
                country: jurisdiction,
            },
            website: None,
            primary_contact,
            authorized_signatories: vec![],
            beneficial_owners: vec![],
            directors: vec![],
            shareholders: vec![],
            regulatory_licenses: vec![],
            banking_relationships: vec![],
            prime_broker_preference: None,
            custodian_preference: None,
            settlement_instructions: ClientSettlementInstructions {
                default_custodian: "".to_string(),
                settlement_cycle: "T+1".to_string(),
                dvp_enabled: true,
                pvp_enabled: true,
                netting_preference: NettingPreference::NetByCurrency,
                fail_tolerance_days: 3,
                cash_sweep_enabled: true,
                collateral_management: true,
                tax_reclaim_service: true,
            },
            investment_objectives: InvestmentObjectives {
                primary_objective: InvestmentObjective::CapitalGrowth,
                secondary_objectives: vec![],
                return_target_pct: 0.15,
                risk_budget_pct: 0.1,
                liquidity_requirements: LiquidityRequirements {
                    daily_liquidity_pct: 0.1,
                    weekly_liquidity_pct: 0.3,
                    monthly_liquidity_pct: 0.5,
                    notice_period_days: 1,
                    redemption_frequency: RedemptionFrequency::Daily,
                },
                esg_requirements: None,
                restricted_investments: vec![],
                mandate_type: MandateType::Discretionary,
            },
            risk_tolerance: RiskTolerance {
                risk_profile: RiskProfile::Balanced,
                max_drawdown_pct: 0.15,
                max_leverage: 5.0,
                var_limit_pct: 0.05,
                concentration_limits: HashMap::new(),
                sector_limits: HashMap::new(),
                currency_limits: HashMap::new(),
                counterparty_limits: HashMap::new(),
                stress_test_requirements: vec![],
            },
            expected_monthly_volume_usd,
            expected_aum_usd,
            source_of_funds: SourceOfFunds {
                primary_source: FundSource::OperatingRevenue,
                secondary_sources: vec![],
                expected_inflows_usd_monthly: expected_monthly_volume_usd / 10,
                expected_outflows_usd_monthly: expected_monthly_volume_usd / 10,
                fund_flow_description: "".to_string(),
                supporting_documents: vec![],
            },
            aml_risk_rating: AMLRiskRating::Medium,
            status: OnboardingStatus::Initiated,
            onboarding_started_at: now,
            onboarding_completed_at: None,
            kyc_completed_at: None,
            kyc_expires_at: None,
            compliance_officer: None,
            legal_counsel: None,
            documents: vec![],
            audit_trail: vec![],
            tags: HashMap::new(),
        };

        self.create_workflow(&client).await;
        self.add_audit_entry(&client, "INITIATE_ONBOARDING", OnboardingStatus::Initiated, "Onboarding initiated").await;
        
        self.clients.write().await.insert(client_id.clone(), client.clone());
        self.update_metrics().await;
        
        info!("Initiated onboarding for client: {}", client_id);
        Ok(client)
    }

    async fn create_workflow(&self, client: &InstitutionalClient) {
        let stages = vec![
            OnboardingStatus::DocumentCollection,
            OnboardingStatus::DocumentVerification,
            OnboardingStatus::KYCReview,
            OnboardingStatus::AMLScreening,
            OnboardingStatus::SanctionsScreening,
            OnboardingStatus::PEPScreening,
            OnboardingStatus::AdverseMediaCheck,
        ];
        
        let mut workflow = WorkflowInstance {
            workflow_id: format!("WF_{}", client.client_id),
            client_id: client.client_id.clone(),
            current_stage: OnboardingStatus::DocumentCollection,
            stage_started_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            stage_sla_hours: self.config.onboarding_sla_hours / stages.len() as u32,
            completed_stages: vec![],
            pending_tasks: stages.iter().map(|s| format!("{:?}", s)).collect(),
            assigned_officers: vec![],
            escalated: false,
            escalation_reason: None,
        };
        
        self.workflow_engine.write().await.active_workflows.insert(workflow.workflow_id.clone(), workflow);
    }

    async fn add_audit_entry(&self, client: &InstitutionalClient, action: &str, stage: OnboardingStatus, details: &str) {
        let entry = AuditEntry {
            entry_id: Uuid::new_v4().to_string(),
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            actor: "SYSTEM".to_string(),
            action: action.to_string(),
            stage,
            details: HashMap::from([("details".to_string(), details.to_string())]),
            ip_address: None,
            user_agent: None,
            hash: format!("{:x}", md5::compute(format!("{}{}{}{}", client.client_id, action, stage as u32, details))),
        };
        
        let mut clients = self.clients.write().await;
        if let Some(c) = clients.get_mut(&client.client_id) {
            c.audit_trail.push(entry);
        }
    }

    pub async fn submit_document(
        &self,
        client_id: &str,
        document: ClientDocument,
    ) -> Result<(), String> {
        let mut clients = self.clients.write().await;
        let client = clients.get_mut(client_id).ok_or("Client not found")?;
        
        client.documents.push(document.clone());
        
        let task = DocumentVerificationTask {
            task_id: Uuid::new_v4().to_string(),
            client_id: client_id.to_string(),
            document_id: document.document_id.clone(),
            document_type: document.document_type,
            assigned_to: None,
            status: VerificationStatus::Queued,
            priority: 5,
            created_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            started_at: None,
            completed_at: None,
            ai_result: None,
            human_result: None,
        };
        
        self.document_queue.write().await.push(task);
        self.add_audit_entry(client, "SUBMIT_DOCUMENT", client.status, &format!("Document submitted: {:?}", document.document_type)).await;
        
        Ok(())
    }

    async fn start_document_processor(&self) {
        let engine = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                engine.process_document_queue().await;
            }
        });
    }

    async fn process_document_queue(&self) {
        let mut queue = self.document_queue.write().await;
        let mut processed = Vec::new();
        
        for (i, task) in queue.iter_mut().enumerate() {
            if task.status == VerificationStatus::Queued {
                task.status = VerificationStatus::AIProcessing;
                task.started_at = Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs());
                
                let ai_result = self.ai_verify_document(task).await;
                task.ai_result = Some(ai_result.clone());
                
                if ai_result.verified && ai_result.confidence > 0.95 {
                    task.status = VerificationStatus::Approved;
                    task.completed_at = Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs());
                } else {
                    task.status = VerificationStatus::HumanReview;
                }
                
                processed.push(i);
            }
        }
        
        for i in processed.iter().rev() {
            let task = queue.remove(*i);
            self.update_client_document_status(&task.client_id, &task.document_id, task.status).await;
        }
    }

    async fn ai_verify_document(&self, task: &DocumentVerificationTask) -> AIVerificationResult {
        AIVerificationResult {
            verified: true,
            confidence: 0.98,
            extracted_data: HashMap::from([
                ("document_type".to_string(), format!("{:?}", task.document_type)),
                ("client_id".to_string(), task.client_id.clone()),
            ]),
            anomalies_detected: vec![],
            model_version: "doc-verify-v2.1".to_string(),
            processed_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        }
    }

    async fn update_client_document_status(&self, client_id: &str, document_id: &str, status: VerificationStatus) {
        let mut clients = self.clients.write().await;
        if let Some(client) = clients.get_mut(client_id) {
            if let Some(doc) = client.documents.iter_mut().find(|d| d.document_id == document_id) {
                match status {
                    VerificationStatus::Approved => {
                        doc.verified_at = Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs());
                        doc.ai_verified = true;
                    }
                    VerificationStatus::Rejected => {
                        doc.verified_at = Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs());
                        doc.ai_verified = false;
                    }
                    _ => {}
                }
            }
        }
    }

    pub async fn advance_workflow_stage(
        &self,
        client_id: &str,
        new_stage: OnboardingStatus,
        officer: &str,
        outcome: StageOutcome,
        notes: String,
    ) -> Result<(), String> {
        let mut workflows = self.workflow_engine.write().await;
        let wf_id = format!("WF_{}", client_id);
        
        if let Some(wf) = workflows.active_workflows.get_mut(&wf_id) {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
            let previous_stage = wf.current_stage.clone();
            
            wf.completed_stages.push(CompletedStage {
                stage: previous_stage.clone(),
                started_at: wf.stage_started_at,
                completed_at: now,
                officer: officer.to_string(),
                outcome: outcome.clone(),
                notes: notes.clone(),
            });
            
            wf.current_stage = new_stage.clone();
            wf.stage_started_at = now;
            
            let clients = self.clients.read().await;
            if let Some(client) = clients.get(client_id) {
                self.add_audit_entry(client, "ADVANCE_STAGE", new_stage.clone(), &format!("{} -> {:?} ({:?})", previous_stage, new_stage, outcome)).await;
            }
            
            if matches!(new_stage, OnboardingStatus::Activated | OnboardingStatus::Rejected) {
                workflows.completed_workflows.push(wf.clone());
                workflows.active_workflows.remove(&wf_id);
            }
        }
        
        let mut clients_mut = self.clients.write().await;
        if let Some(client) = clients_mut.get_mut(client_id) {
            if let Some(wf) = workflows.active_workflows.get(&wf_id) {
                client.status = wf.current_stage.clone();
                if wf.current_stage == OnboardingStatus::Activated {
                    client.onboarding_completed_at = Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs());
                    client.kyc_completed_at = Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs());
                    client.kyc_expires_at = Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() + 365 * 24 * 3600);
                }
            }
        }
        
        Ok(())
    }

    pub async fn setup_prime_broker_account(
        &self,
        client_id: &str,
        prime_broker_id: &str,
        account_type: String,
        credit_limit_usd: f64,
    ) -> Result<PrimeBrokerAccount, String> {
        let pb_config = self.config.prime_broker_integrations.iter()
            .find(|p| p.broker_id == prime_broker_id && p.enabled)
            .ok_or("Prime broker not found or not enabled")?;
        
        let account = PrimeBrokerAccount {
            account_id: format!("PB_{}_{}", prime_broker_id, Uuid::new_v4().to_string()[..8].to_uppercase()),
            client_id: client_id.to_string(),
            prime_broker_id: prime_broker_id.to_string(),
            account_type,
            margin_agreement_signed: false,
            margin_agreement_date: None,
            credit_limit_usd,
            used_margin_usd: 0.0,
            available_margin_usd: credit_limit_usd,
            margin_rate: pb_config.margin_rates.values().cloned().next().unwrap_or(0.02),
            custody_fee_bps: pb_config.custody_fee_bps,
            settlement_cycle: pb_config.settlement_cycle.clone(),
            supported_products: pb_config.supported_assets.clone(),
            reporting_frequency: ReportingFrequency::Daily,
            status: AccountStatus::Pending,
            opened_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            last_reviewed_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        };
        
        self.prime_broker_accounts.write().await.insert(account.account_id.clone(), account.clone());
        
        let mut clients = self.clients.write().await;
        if let Some(client) = clients.get_mut(client_id) {
            client.prime_broker_preference = Some(prime_broker_id.to_string());
        }
        
        self.update_metrics().await;
        Ok(account)
    }

    pub async fn setup_custodian_account(
        &self,
        client_id: &str,
        custodian_id: &str,
        account_name: String,
        base_currency: String,
    ) -> Result<CustodianAccount, String> {
        let cust_config = self.config.custodian_integrations.iter()
            .find(|c| c.custodian_id == custodian_id && c.enabled)
            .ok_or("Custodian not found or not enabled")?;
        
        let account = CustodianAccount {
            account_id: format!("CUST_{}_{}", custodian_id, Uuid::new_v4().to_string()[..8].to_uppercase()),
            client_id: client_id.to_string(),
            custodian_id: custodian_id.to_string(),
            account_name,
            base_currency,
            supported_currencies: cust_config.supported_markets.clone(),
            settlement_systems: cust_config.settlement_systems.clone(),
            fee_schedule: FeeSchedule {
                custody_fee_bps: cust_config.fee_bps,
                transaction_fee_usd: 5.0,
                fx_fee_bps: 10,
                corporate_action_fee_usd: 25.0,
                proxy_voting_fee_usd: 10.0,
                minimum_fee_usd_monthly: 1000.0,
            },
            tax_services: TaxServices {
                withholding_tax_reclaim: true,
                tax_reporting: true,
                certificate_management: true,
                treaty_benefits: true,
            },
            corporate_actions: CorporateActionServices {
                mandatory_actions: true,
                voluntary_actions: true,
                voting_rights: true,
                dividend_processing: true,
            },
            proxy_voting: true,
            securities_lending: SecuritiesLendingConfig {
                enabled: true,
                revenue_split_pct: 0.7,
                collateral_requirements: CollateralRequirements {
                    acceptable_collateral: vec!["Cash".to_string(), "GovernmentBonds".to_string(), "Equities".to_string()],
                    haircut_schedule: HashMap::from([
                        ("Cash".to_string(), 0.0),
                        ("GovernmentBonds".to_string(), 0.02),
                        ("Equities".to_string(), 0.15),
                    ]),
                    concentration_limits: HashMap::from([
                        ("Cash".to_string(), 1.0),
                        ("GovernmentBonds".to_string(), 0.8),
                        ("Equities".to_string(), 0.5),
                    ]),
                    currency_matching_required: true,
                },
                borrower_restrictions: vec![],
                minimum_loan_size_usd: 100_000.0,
            },
            status: AccountStatus::Pending,
            opened_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        };
        
        self.custodian_accounts.write().await.insert(account.account_id.clone(), account.clone());
        
        let mut clients = self.clients.write().await;
        if let Some(client) = clients.get_mut(client_id) {
            client.custodian_preference = Some(custodian_id.to_string());
            client.settlement_instructions.default_custodian = custodian_id.to_string();
        }
        
        self.update_metrics().await;
        Ok(account)
    }

    async fn start_workflow_processor(&self) {
        let engine = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                engine.process_workflows().await;
            }
        });
    }

    async fn process_workflows(&self) {
        let mut workflows = self.workflow_engine.write().await;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        
        let workflow_ids: Vec<String> = workflows.active_workflows.keys().cloned().collect();
        
        for wf_id in workflow_ids {
            if let Some(wf) = workflows.active_workflows.get_mut(&wf_id) {
                let stage_elapsed = now - wf.stage_started_at;
                let sla_seconds = wf.stage_sla_hours as u64 * 3600;
                
                if stage_elapsed > sla_seconds && !wf.escalated {
                    wf.escalated = true;
                    wf.escalation_reason = Some(format!("SLA breach: stage {:?} exceeded {} hours", wf.current_stage, wf.stage_sla_hours));
                    warn!("Workflow {} escalated: {}", wf_id, wf.escalation_reason.as_ref().unwrap());
                }
            }
        }
    }

    async fn start_sla_monitor(&self) {
        let engine = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3600));
            loop {
                interval.tick().await;
                engine.update_metrics().await;
            }
        });
    }

    async fn update_metrics(&self) {
        let clients = self.clients.read().await;
        let pb_accounts = self.prime_broker_accounts.read().await;
        let cust_accounts = self.custodian_accounts.read().await;
        let queue = self.document_queue.read().await;
        let workflows = self.workflow_engine.read().await;
        
        let mut metrics = self.metrics.write().await;
        metrics.total_clients = clients.len() as u32;
        metrics.active_onboardings = workflows.active_workflows.len() as u32;
        metrics.documents_pending = queue.iter().filter(|t| t.status != VerificationStatus::Approved).count() as u32;
        metrics.prime_broker_accounts = pb_accounts.len() as u32;
        metrics.custodian_accounts = cust_accounts.len() as u32;
        metrics.total_aum_usd = clients.values().map(|c| c.expected_aum_usd).sum();
        metrics.total_expected_volume_usd = clients.values().map(|c| c.expected_monthly_volume_usd).sum();
        
        let completed: Vec<_> = workflows.completed_workflows.iter()
            .filter(|w| w.completed_stages.last().map(|s| s.completed_at).unwrap_or(0) > SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() - 2592000)
            .collect();
        
        metrics.completed_this_month = completed.len() as u32;
        
        if !completed.is_empty() {
            let total_time: u64 = completed.iter()
                .map(|w| w.completed_stages.last().unwrap().completed_at - w.stage_started_at)
                .sum();
            metrics.avg_onboarding_time_hours = (total_time as f64 / completed.len() as f64) / 3600.0;
        }
    }

    pub async fn get_client(&self, client_id: &str) -> Option<InstitutionalClient> {
        self.clients.read().await.get(client_id).cloned()
    }

    pub async fn get_workflow(&self, client_id: &str) -> Option<WorkflowInstance> {
        self.workflow_engine.read().await.active_workflows.get(&format!("WF_{}", client_id)).cloned()
    }

    pub async fn get_prime_broker_account(&self, account_id: &str) -> Option<PrimeBrokerAccount> {
        self.prime_broker_accounts.read().await.get(account_id).cloned()
    }

    pub async fn get_custodian_account(&self, account_id: &str) -> Option<CustodianAccount> {
        self.custodian_accounts.read().await.get(account_id).cloned()
    }

    pub async fn get_metrics(&self) -> OnboardingMetrics {
        self.metrics.read().await.clone()
    }

    pub async fn list_clients(&self, status: Option<OnboardingStatus>) -> Vec<InstitutionalClient> {
        let clients = self.clients.read().await;
        clients.values()
            .filter(|c| status.as_ref().map_or(true, |s| &c.status == s))
            .cloned()
            .collect()
    }
}

impl Clone for OnboardingEngine {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            clients: self.clients.clone(),
            prime_broker_accounts: self.prime_broker_accounts.clone(),
            custodian_accounts: self.custodian_accounts.clone(),
            document_queue: self.document_queue.clone(),
            workflow_engine: self.workflow_engine.clone(),
            metrics: self.metrics.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_onboarding_engine() {
        let engine = OnboardingEngine::new(OnboardingConfig::default());
        engine.start().await.unwrap();
        
        let contact = Contact {
            name: "John Doe".to_string(),
            title: "CEO".to_string(),
            email: "john@fund.com".to_string(),
            phone: "+1-555-1234".to_string(),
            preferred_contact_method: ContactMethod::Email,
        };
        
        let client = engine.initiate_onboarding(
            "Test Fund LP".to_string(),
            EntityType::Fund,
            "AE".to_string(),
            contact,
            50_000_000,
            500_000_000,
        ).await.unwrap();
        
        assert_eq!(client.status, OnboardingStatus::Initiated);
        assert_eq!(client.expected_aum_usd, 500_000_000);
        
        let pb_account = engine.link_prime_broker(&client.client_id, "GOLDMAN_SACHS").await.unwrap();
        assert_eq!(pb_account.prime_broker_id, "GOLDMAN_SACHS");
        
        let cust_account = engine.link_custodian(&client.client_id, "BNY_MELLON").await.unwrap();
        assert_eq!(cust_account.custodian_id, "BNY_MELLON");
    }
}