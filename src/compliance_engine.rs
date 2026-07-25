use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tracing::{info, warn, error};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceConfig {
    pub kyc_required: bool,
    pub aml_threshold_usd: u64,
    pub sanctions_screening_enabled: bool,
    pub transaction_monitoring_enabled: bool,
    pub regulatory_reporting_enabled: bool,
    pub reporting_jurisdictions: Vec<String>,
    pub pep_screening_enabled: bool,
    pub adverse_media_screening: bool,
    pub travel_rule_threshold_usd: u64,
    pub large_transaction_reporting_threshold_usd: u64,
    pub suspicious_activity_threshold_usd: u64,
    pub max_daily_volume_per_client_usd: u64,
    pub max_position_size_pct: f64,
    pub restricted_jurisdictions: Vec<String>,
    pub allowed_jurisdictions: Vec<String>,
    pub auto_freeze_suspicious: bool,
    pub compliance_action: bool,
    pub audit_log_retention_days: u32,
}

impl Default for ComplianceConfig {
    fn default() -> Self {
        Self {
            kyc_required: true,
            aml_threshold_usd: 10_000,
            sanctions_screening_enabled: true,
            transaction_monitoring_enabled: true,
            regulatory_reporting_enabled: true,
            reporting_jurisdictions: vec!["US".to_string(), "EU".to_string(), "UK".to_string(), "AE".to_string(), "SG".to_string()],
            pep_screening_enabled: true,
            adverse_media_screening: true,
            travel_rule_threshold_usd: 3_000,
            large_transaction_reporting_threshold_usd: 10_000,
            suspicious_activity_threshold_usd: 50_000,
            max_daily_volume_per_client_usd: 1_000_000_000,
            max_position_size_pct: 0.1,
            restricted_jurisdictions: vec!["KP".to_string(), "IR".to_string(), "SY".to_string(), "CU".to_string()],
            allowed_jurisdictions: vec!["US".to_string(), "GB".to_string(), "DE".to_string(), "FR".to_string(), "AE".to_string(), "SG".to_string(), "HK".to_string(), "CH".to_string()],
            auto_freeze_suspicious: true,
            compliance_action: true,
            audit_log_retention_days: 2555,
        }
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
    Prohibited,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantComplianceProfile {
    pub participant_id: String,
    pub legal_entity_name: String,
    pub jurisdiction: String,
    pub lei: Option<String>,
    pub kyc_status: KYCStatus,
    pub kyc_completed_at: Option<u64>,
    pub kyc_expires_at: Option<u64>,
    pub risk_level: RiskLevel,
    pub risk_score: f64,
    pub sanctions_match: bool,
    pub pep_match: bool,
    pub adverse_media_hits: u32,
    pub aml_alerts_count: u32,
    pub total_volume_usd: u64,
    pub daily_volume_usd: u64,
    pub last_volume_reset: u64,
    pub frozen: bool,
    pub freeze_reason: Option<String>,
    pub compliance_officer_notes: Vec<ComplianceNote>,
    pub documents: Vec<KYCDocument>,
    pub beneficial_owners: Vec<BeneficialOwner>,
    pub regulatory_registrations: Vec<RegulatoryRegistration>,
    pub travel_rule_counterparties: Vec<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KYCDocument {
    pub document_id: String,
    pub document_type: DocumentType,
    pub file_hash: String,
    pub issued_at: u64,
    pub expires_at: Option<u64>,
    pub issuing_authority: String,
    pub verified: bool,
    pub verified_at: Option<u64>,
    pub verified_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DocumentType {
    CertificateOfIncorporation,
    BusinessLicense,
    ArticlesOfAssociation,
    RegisterOfDirectors,
    RegisterOfShareholders,
    BeneficialOwnershipDeclaration,
    ProofOfAddress,
    TaxResidencyCertificate,
    RegulatoryLicense,
    LEICertificate,
    AuditedFinancialStatements,
    InsuranceCertificate,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeneficialOwner {
    pub owner_id: String,
    pub name: String,
    pub ownership_pct: f64,
    pub nationality: String,
    pub country_of_residence: String,
    pub date_of_birth: u64,
    pub pep_status: bool,
    pub sanctions_check: bool,
    pub kyc_status: KYCStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegulatoryRegistration {
    pub regulator: String,
    pub registration_number: String,
    pub registration_type: String,
    pub issued_at: u64,
    pub expires_at: Option<u64>,
    pub status: String,
    pub jurisdiction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceNote {
    pub note_id: String,
    pub author: String,
    pub timestamp: u64,
    pub note_type: NoteType,
    pub content: String,
    pub severity: RiskLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NoteType {
    KYCReview,
    AMLAlert,
    SanctionsHit,
    PEPMatch,
    AdverseMedia,
    TransactionReview,
    RegulatoryInquiry,
    FreezeAction,
    UnfreezeAction,
    PeriodicReview,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceAlert {
    pub alert_id: String,
    pub participant_id: String,
    pub alert_type: AlertType,
    pub severity: RiskLevel,
    pub description: String,
    pub triggered_at: u64,
    pub acknowledged: bool,
    pub acknowledged_at: Option<u64>,
    pub acknowledged_by: Option<String>,
    pub resolved: bool,
    pub resolved_at: Option<u64>,
    pub resolution: Option<String>,
    pub related_transactions: Vec<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlertType {
    SanctionsMatch,
    PEPMatch,
    AdverseMedia,
    LargeTransaction,
    StructuredTransaction,
    VelocityThreshold,
    GeographyRisk,
    CounterpartyRisk,
    UnusualPattern,
    TravelRuleViolation,
    KYCExpired,
    DocumentExpired,
    RegulatoryDeadline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegulatoryReport {
    pub report_id: String,
    pub report_type: ReportType,
    pub jurisdiction: String,
    pub reporting_period_start: u64,
    pub reporting_period_end: u64,
    pub generated_at: u64,
    pub submitted_at: Option<u64>,
    pub status: ReportStatus,
    pub transactions: Vec<ReportableTransaction>,
    pub summary: ReportSummary,
    pub file_hash: Option<String>,
    pub submission_reference: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReportType {
    CTR,
    SAR,
    FormPF,
    EMIR,
    MiFID,
    MAS,
    DFSA,
    ADGM,
    VARA,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReportStatus {
    Draft,
    PendingReview,
    Approved,
    Submitted,
    Acknowledged,
    Rejected,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportableTransaction {
    pub transaction_id: String,
    pub timestamp: u64,
    pub participant_id: String,
    pub counterparty_id: String,
    pub symbol: String,
    pub side: String,
    pub quantity: f64,
    pub price: f64,
    pub notional_usd: f64,
    pub currency: String,
    pub transaction_type: String,
    pub reporting_obligation: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummary {
    pub total_transactions: u32,
    pub total_volume_usd: u64,
    pub unique_participants: u32,
    pub ctr_count: u32,
    pub sar_count: u32,
    pub large_transactions: u32,
    pub cross_border_transactions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub log_id: String,
    pub timestamp: u64,
    pub actor: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub details: HashMap<String, String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub result: AuditResult,
    pub risk_level: RiskLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuditResult {
    Success,
    Failure,
    Partial,
    Blocked,
}

pub struct ComplianceEngine {
    config: ComplianceConfig,
    profiles: Arc<RwLock<HashMap<String, ParticipantComplianceProfile>>>,
    alerts: Arc<RwLock<Vec<ComplianceAlert>>>,
    reports: Arc<RwLock<Vec<RegulatoryReport>>>,
    audit_log: Arc<RwLock<Vec<AuditLogEntry>>>,
    sanctions_lists: Arc<RwLock<HashMap<String, SanctionsEntry>>>,
    pep_database: Arc<RwLock<HashMap<String, PEPEntry>>>,
    adverse_media_cache: Arc<RwLock<HashMap<String, AdverseMediaEntry>>>,
    transaction_monitor: Arc<RwLock<TransactionMonitor>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanctionsEntry {
    pub entity_id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub list_source: String,
    pub list_date: u64,
    pub programs: Vec<String>,
    pub addresses: Vec<String>,
    pub identifiers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PEPEntry {
    pub person_id: String,
    pub name: String,
    pub position: String,
    pub country: String,
    pub start_date: u64,
    pub end_date: Option<u64>,
    pub risk_level: RiskLevel,
    pub source: String,
    pub relatives: Vec<String>,
    pub associates: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdverseMediaEntry {
    pub entity_id: String,
    pub name: String,
    pub articles: Vec<AdverseMediaArticle>,
    pub last_updated: u64,
    pub risk_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdverseMediaArticle {
    pub article_id: String,
    pub title: String,
    pub url: String,
    pub source: String,
    pub published_at: u64,
    pub sentiment: f64,
    pub categories: Vec<String>,
    pub relevance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TransactionMonitor {
    pub velocity_windows: HashMap<String, VelocityWindow>,
    pub pattern_cache: HashMap<String, PatternMatch>,
    pub daily_volumes: HashMap<String, u64>,
    pub counterparty_exposure: HashMap<String, HashMap<String, f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VelocityWindow {
    pub window_start: u64,
    pub window_end: u64,
    pub transaction_count: u32,
    pub total_volume_usd: u64,
    pub unique_counterparties: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternMatch {
    pub pattern_id: String,
    pub pattern_type: String,
    pub confidence: f64,
    pub detected_at: u64,
    pub transactions: Vec<String>,
}

impl ComplianceEngine {
    pub fn new(config: ComplianceConfig) -> Self {
        Self {
            config,
            profiles: Arc::new(RwLock::new(HashMap::new())),
            alerts: Arc::new(RwLock::new(Vec::new())),
            reports: Arc::new(RwLock::new(Vec::new())),
            audit_log: Arc::new(RwLock::new(Vec::new())),
            sanctions_lists: Arc::new(RwLock::new(HashMap::new())),
            pep_database: Arc::new(RwLock::new(HashMap::new())),
            adverse_media_cache: Arc::new(RwLock::new(HashMap::new())),
            transaction_monitor: Arc::new(RwLock::new(TransactionMonitor::default())),
        }
    }

    pub async fn initialize(&self) -> Result<(), String> {
        self.load_sanctions_lists().await?;
        self.load_pep_database().await?;
        self.start_monitoring_tasks().await;
        info!("Compliance engine initialized");
        Ok(())
    }

    async fn load_sanctions_lists(&self) -> Result<(), String> {
        let mut lists = self.sanctions_lists.write().await;
        lists.insert("OFAC_SDN".to_string(), SanctionsEntry {
            entity_id: "OFAC_SDN".to_string(),
            name: "OFAC Specially Designated Nationals".to_string(),
            aliases: vec![],
            list_source: "US Treasury OFAC".to_string(),
            list_date: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            programs: vec!["SDN".to_string(), "FSE".to_string()],
            addresses: vec![],
            identifiers: vec![],
        });
        lists.insert("EU_CONSOLIDATED".to_string(), SanctionsEntry {
            entity_id: "EU_CONSOLIDATED".to_string(),
            name: "EU Consolidated Sanctions List".to_string(),
            aliases: vec![],
            list_source: "EU Council".to_string(),
            list_date: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            programs: vec!["EU_FINANCIAL".to_string()],
            addresses: vec![],
            identifiers: vec![],
        });
        lists.insert("UN_CONSOLIDATED".to_string(), SanctionsEntry {
            entity_id: "UN_CONSOLIDATED".to_string(),
            name: "UN Security Council Consolidated List".to_string(),
            aliases: vec![],
            list_source: "United Nations".to_string(),
            list_date: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            programs: vec!["UN_SANCTIONS".to_string()],
            addresses: vec![],
            identifiers: vec![],
        });
        info!("Loaded {} sanctions lists", lists.len());
        Ok(())
    }

    async fn load_pep_database(&self) -> Result<(), String> {
        let mut db = self.pep_database.write().await;
        db.insert("PEP_001".to_string(), PEPEntry {
            person_id: "PEP_001".to_string(),
            name: "Test PEP".to_string(),
            position: "Minister of Finance".to_string(),
            country: "AE".to_string(),
            start_date: 1609459200,
            end_date: None,
            risk_level: RiskLevel::High,
            source: "Public Registry".to_string(),
            relatives: vec![],
            associates: vec![],
        });
        info!("Loaded {} PEP entries", db.len());
        Ok(())
    }

    async fn start_monitoring_tasks(&self) {
        let engine = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600));
            loop {
                interval.tick().await;
                engine.daily_volume_reset().await;
                engine.generate_periodic_reports().await;
            }
        });

        let engine = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300));
            loop {
                interval.tick().await;
                engine.process_alert_queue().await;
            }
        });
    }

    pub async fn register_participant(
        &self,
        participant_id: String,
        legal_entity_name: String,
        jurisdiction: String,
        lei: Option<String>,
    ) -> Result<ParticipantComplianceProfile, String> {
        if self.config.restricted_jurisdictions.contains(&jurisdiction) {
            return Err(format!("Jurisdiction {} is restricted", jurisdiction));
        }

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let profile = ParticipantComplianceProfile {
            participant_id: participant_id.clone(),
            legal_entity_name,
            jurisdiction: jurisdiction.clone(),
            lei,
            kyc_status: if self.config.kyc_required { KYCStatus::NotStarted } else { KYCStatus::Approved },
            kyc_completed_at: None,
            kyc_expires_at: None,
            risk_level: RiskLevel::Low,
            risk_score: 0.1,
            sanctions_match: false,
            pep_match: false,
            adverse_media_hits: 0,
            aml_alerts_count: 0,
            total_volume_usd: 0,
            daily_volume_usd: 0,
            last_volume_reset: now,
            frozen: false,
            freeze_reason: None,
            compliance_officer_notes: vec![],
            documents: vec![],
            beneficial_owners: vec![],
            regulatory_registrations: vec![],
            travel_rule_counterparties: vec![],
            created_at: now,
            updated_at: now,
        };

        self.run_initial_screening(&profile).await;
        self.profiles.write().await.insert(participant_id.clone(), profile.clone());
        self.audit_log(participant_id, participant_id, "REGISTER_PARTICIPANT", "compliance_profile", &participant_id, AuditResult::Success, RiskLevel::Low).await;
        
        info!("Registered participant {} for compliance", participant_id);
        Ok(profile)
    }

    async fn run_initial_screening(&self, profile: &ParticipantComplianceProfile) {
        if self.config.sanctions_screening_enabled {
            self.screen_sanctions(&profile.participant_id, &profile.legal_entity_name).await;
        }
        if self.config.pep_screening_enabled {
            self.screen_pep(&profile.participant_id, &profile.beneficial_owners).await;
        }
        if self.config.adverse_media_screening {
            self.screen_adverse_media(&profile.participant_id, &profile.legal_entity_name).await;
        }
    }

    pub async fn submit_kyc_documents(
        &self,
        participant_id: &str,
        documents: Vec<KYCDocument>,
    ) -> Result<(), String> {
        let mut profiles = self.profiles.write().await; let profile = profiles.get_mut(participant_id).ok_or("Participant not found")?;
        
        for doc in documents {
            if !profile.documents.iter().any(|d| d.document_id == doc.document_id) {
                profile.documents.push(doc);
            }
        }
        
        profile.kyc_status = KYCStatus::PendingReview;
        profile.updated_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        
        self.audit_log(participant_id, participant_id, "SUBMIT_KYC_DOCS", "kyc_documents", participant_id, AuditResult::Success, RiskLevel::Low).await;
        Ok(())
    }

    pub async fn review_kyc(
        &self,
        participant_id: &str,
        reviewer: &str,
        approved: bool,
        notes: String,
    ) -> Result<(), String> {
        let mut profiles = self.profiles.write().await; let profile = profiles.get_mut(participant_id).ok_or("Participant not found")?;
        
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        
        if approved {
            profile.kyc_status = KYCStatus::Approved;
            profile.kyc_completed_at = Some(now);
            profile.kyc_expires_at = Some(now + 365 * 24 * 3600);
            profile.risk_level = self.calculate_risk_level(&profile).await;
            
            for doc in &mut profile.documents {
                doc.verified = true;
                doc.verified_at = Some(now);
                doc.verified_by = Some(reviewer.to_string());
            }
        } else {
            profile.kyc_status = KYCStatus::Rejected;
        }
        
        profile.compliance_officer_notes.push(ComplianceNote {
            note_id: Uuid::new_v4().to_string(),
            author: reviewer.to_string(),
            timestamp: now,
            note_type: NoteType::KYCReview,
            content: notes,
            severity: if approved { RiskLevel::Low } else { RiskLevel::High },
        });
        
        profile.updated_at = now;
        self.audit_log(participant_id, participant_id, "REVIEW_KYC", "kyc_status", participant_id, 
            if approved { AuditResult::Success } else { AuditResult::Failure }, 
            if approved { RiskLevel::Low } else { RiskLevel::High }
        ).await;
        
        Ok(())
    }

    async fn calculate_risk_level(&self, profile: &ParticipantComplianceProfile) -> RiskLevel {
        let mut score = 0.0;
        
        if profile.sanctions_match { score += 1.0; }
        if profile.pep_match { score += 0.5; }
        score += profile.adverse_media_hits as f64 * 0.1;
        score += profile.aml_alerts_count as f64 * 0.2;
        
        if profile.jurisdiction != "US" && profile.jurisdiction != "GB" && profile.jurisdiction != "AE" {
            score += 0.3;
        }
        
        for owner in &profile.beneficial_owners {
            if owner.pep_status { score += 0.3; }
            if owner.sanctions_check { score += 0.5; }
        }
        
        profile.risk_score = score.min(1.0);
        
        match score {
            s if s >= 0.8 => RiskLevel::Critical,
            s if s >= 0.6 => RiskLevel::High,
            s if s >= 0.3 => RiskLevel::Medium,
            _ => RiskLevel::Low,
        }
    }

    pub async fn screen_transaction(
        &self,
        participant_id: &str,
        counterparty_id: &str,
        symbol: &str,
        side: &str,
        quantity: f64,
        price: f64,
        notional_usd: f64,
    ) -> Result<Vec<ComplianceAlert>, String> {
        let mut alerts = Vec::new();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        
        let profile = self.profiles.read().await.get(participant_id).cloned().ok_or("Participant not found")?;
        
        if profile.frozen {
            alerts.push(self.create_alert(participant_id, AlertType::UnusualPattern, RiskLevel::Critical,
                "Trading attempted on frozen account".to_string(), vec![]).await);
            return Ok(alerts);
        }

        if self.config.restricted_jurisdictions.contains(&profile.jurisdiction) {
            alerts.push(self.create_alert(participant_id, AlertType::GeographyRisk, RiskLevel::Critical,
                format!("Participant from restricted jurisdiction: {}", profile.jurisdiction), vec![]).await);
        }

        if notional_usd >= self.config.large_transaction_reporting_threshold_usd as f64 {
            alerts.push(self.create_alert(participant_id, AlertType::LargeTransaction, RiskLevel::Medium,
                format!("Large transaction: ${:.2}", notional_usd), vec![]).await);
        }

        if notional_usd >= self.config.suspicious_activity_threshold_usd as f64 {
            alerts.push(self.create_alert(participant_id, AlertType::UnusualPattern, RiskLevel::High,
                format!("Suspicious activity threshold exceeded: ${:.2}", notional_usd), vec![]).await);
        }

        if profile.daily_volume_usd + notional_usd as u64 > self.config.max_daily_volume_per_client_usd {
            alerts.push(self.create_alert(participant_id, AlertType::VelocityThreshold, RiskLevel::High,
                "Daily volume limit would be exceeded".to_string(), vec![]).await);
        }

        let counterparty_profile = self.profiles.read().await.get(counterparty_id).cloned();
        if let Some(cp) = counterparty_profile {
            if cp.frozen {
                alerts.push(self.create_alert(participant_id, AlertType::CounterpartyRisk, RiskLevel::High,
                    "Counterparty account is frozen".to_string(), vec![]).await);
            }
            if cp.sanctions_match {
                alerts.push(self.create_alert(participant_id, AlertType::SanctionsMatch, RiskLevel::Critical,
                    "Counterparty has sanctions match".to_string(), vec![]).await);
            }
        }

        if notional_usd >= self.config.travel_rule_threshold_usd as f64 {
            if !profile.travel_rule_counterparties.contains(&counterparty_id.to_string()) {
                alerts.push(self.create_alert(participant_id, AlertType::TravelRuleViolation, RiskLevel::Medium,
                    "Travel rule information required for counterparty".to_string(), vec![]).await);
            }
        }

        self.update_transaction_monitoring(participant_id, counterparty_id, notional_usd as u64, now).await;
        
        if !alerts.is_empty() {
            let mut all_alerts = self.alerts.write().await;
            all_alerts.extend(alerts.clone());
        }

        Ok(alerts)
    }

    async fn update_transaction_monitoring(
        &self,
        participant_id: &str,
        counterparty_id: &str,
        volume_usd: u64,
        timestamp: u64,
    ) {
        let mut monitor = self.transaction_monitor.write().await;
        
        let day_start = timestamp - (timestamp % 86400);
        let window_key = format!("{}_{}", participant_id, day_start);
        
        let window = monitor.velocity_windows.entry(window_key).or_insert(VelocityWindow {
            window_start: day_start,
            window_end: day_start + 86400,
            transaction_count: 0,
            total_volume_usd: 0,
            unique_counterparties: 0,
        });
        
        window.transaction_count += 1;
        window.total_volume_usd += volume_usd;
        
        let cp_key = format!("{}_{}", participant_id, counterparty_id);
        *monitor.counterparty_exposure.entry(participant_id.to_string()).or_default()
            .entry(counterparty_id.to_string()).or_insert(0.0) += volume_usd as f64;
        
        *monitor.daily_volumes.entry(participant_id.to_string()).or_insert(0) += volume_usd;
        
        if let Some(profile) = self.profiles.write().await.get_mut(participant_id) {
            profile.daily_volume_usd += volume_usd;
            profile.total_volume_usd += volume_usd;
            profile.updated_at = timestamp;
        }
    }

    async fn screen_sanctions(&self, participant_id: &str, name: &str) {
        let lists = self.sanctions_lists.read().await;
        for entry in lists.values() {
            if entry.name.to_lowercase().contains(&name.to_lowercase()) ||
               entry.aliases.iter().any(|a| a.to_lowercase().contains(&name.to_lowercase())) {
                self.create_alert(participant_id, AlertType::SanctionsMatch, RiskLevel::Critical,
                    format!("Sanctions match found: {} in {}", name, entry.list_source), vec![]).await;
                
                if let Some(profile) = self.profiles.write().await.get_mut(participant_id) {
                    profile.sanctions_match = true;
                    profile.risk_level = RiskLevel::Critical;
                    if self.config.auto_freeze_suspicious {
                        profile.frozen = true;
                        profile.freeze_reason = Some("Sanctions match".to_string());
                    }
                }
                break;
            }
        }
    }

    async fn screen_pep(&self, participant_id: &str, beneficial_owners: &[BeneficialOwner]) {
        let db = self.pep_database.read().await;
        for owner in beneficial_owners {
            for entry in db.values() {
                if entry.name.to_lowercase() == owner.name.to_lowercase() {
                    self.create_alert(participant_id, AlertType::PEPMatch, RiskLevel::High,
                        format!("PEP match for beneficial owner: {}", owner.name), vec![]).await;
                    
                    if let Some(profile) = self.profiles.write().await.get_mut(participant_id) {
                        profile.pep_match = true;
                    }
                    break;
                }
            }
        }
    }

    async fn screen_adverse_media(&self, participant_id: &str, name: &str) {
        let cache = self.adverse_media_cache.read().await;
        if let Some(entry) = cache.get(name) {
            if entry.risk_score > 0.7 {
                self.create_alert(participant_id, AlertType::AdverseMedia, RiskLevel::High,
                    format!("Adverse media detected for {}: {} articles", name, entry.articles.len()), vec![]).await;
                
                if let Some(profile) = self.profiles.write().await.get_mut(participant_id) {
                    profile.adverse_media_hits = entry.articles.len() as u32;
                }
            }
        }
    }

    async fn create_alert(
        &self,
        participant_id: &str,
        alert_type: AlertType,
        severity: RiskLevel,
        description: String,
        related_transactions: Vec<String>,
    ) -> ComplianceAlert {
        ComplianceAlert {
            alert_id: Uuid::new_v4().to_string(),
            participant_id: participant_id.to_string(),
            alert_type,
            severity,
            description,
            triggered_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            acknowledged: false,
            acknowledged_at: None,
            acknowledged_by: None,
            resolved: false,
            resolved_at: None,
            resolution: None,
            related_transactions,
            metadata: HashMap::new(),
        }
    }

    pub async fn acknowledge_alert(&self, alert_id: &str, acknowledged_by: &str) -> Result<(), String> {
        let mut alerts = self.alerts.write().await;
        if let Some(alert) = alerts.iter_mut().find(|a| a.alert_id == alert_id) {
            alert.acknowledged = true;
            alert.acknowledged_at = Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs());
            alert.acknowledged_by = Some(acknowledged_by.to_string());
            Ok(())
        } else {
            Err("Alert not found".to_string())
        }
    }

    pub async fn resolve_alert(&self, alert_id: &str, resolved_by: &str, resolution: String) -> Result<(), String> {
        let mut alerts = self.alerts.write().await;
        if let Some(alert) = alerts.iter_mut().find(|a| a.alert_id == alert_id) {
            alert.resolved = true;
            alert.resolved_at = Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs());
            alert.resolution = Some(resolution);
            Ok(())
        } else {
            Err("Alert not found".to_string())
        }
    }

    pub async fn get_alerts(&self, participant_id: Option<&str>, unresolved_only: bool) -> Vec<ComplianceAlert> {
        let alerts = self.alerts.read().await;
        alerts.iter()
            .filter(|a| participant_id.map_or(true, |p| a.participant_id == p))
            .filter(|a| !unresolved_only || !a.resolved)
            .cloned()
            .collect()
    }

    pub async fn freeze_participant(&self, participant_id: &str, reason: String, officer: &str) -> Result<(), String> {
        let mut profiles = self.profiles.write().await; let profile = profiles.get_mut(participant_id).ok_or("Participant not found")?;
        profile.frozen = true;
        profile.freeze_reason = Some(reason.clone());
        profile.compliance_officer_notes.push(ComplianceNote {
            note_id: Uuid::new_v4().to_string(),
            author: officer.to_string(),
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            note_type: NoteType::FreezeAction,
            content: reason,
            severity: RiskLevel::Critical,
        });
        self.audit_log(participant_id, participant_id, "FREEZE_PARTICIPANT", "compliance_profile", participant_id, AuditResult::Success, RiskLevel::Critical).await;
        Ok(())
    }

    pub async fn unfreeze_participant(&self, participant_id: &str, reason: String, officer: &str) -> Result<(), String> {
        let mut profiles = self.profiles.write().await; let profile = profiles.get_mut(participant_id).ok_or("Participant not found")?;
        profile.frozen = false;
        profile.freeze_reason = None;
        profile.compliance_officer_notes.push(ComplianceNote {
            note_id: Uuid::new_v4().to_string(),
            author: officer.to_string(),
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            note_type: NoteType::UnfreezeAction,
            content: reason,
            severity: RiskLevel::Low,
        });
        self.audit_log(participant_id, participant_id, "UNFREEZE_PARTICIPANT", "compliance_profile", participant_id, AuditResult::Success, RiskLevel::Low).await;
        Ok(())
    }

    async fn daily_volume_reset(&self) {
        let mut profiles = self.profiles.write().await;
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        for profile in profiles.values_mut() {
            profile.daily_volume_usd = 0;
            profile.last_volume_reset = now;
        }
        let mut monitor = self.transaction_monitor.write().await;
        monitor.daily_volumes.clear();
        info!("Daily volume reset completed");
    }

    async fn generate_periodic_reports(&self) {
        if !self.config.regulatory_reporting_enabled { return; }
        
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let period_start = now - 86400;
        
        for jurisdiction in &self.config.reporting_jurisdictions {
            let report = self.generate_regulatory_report(jurisdiction, period_start, now).await;
            self.reports.write().await.push(report);
        }
    }

    async fn generate_regulatory_report(
        &self,
        jurisdiction: &str,
        period_start: u64,
        period_end: u64,
    ) -> RegulatoryReport {
        let profiles = self.profiles.read().await;
        let alerts = self.alerts.read().await;
        
        let mut transactions = Vec::new();
        let mut total_volume = 0u64;
        let mut unique_participants = std::collections::HashSet::new();
        let mut ctr_count = 0;
        let mut sar_count = 0;
        let mut large_transactions = 0;
        
        for profile in profiles.values() {
            unique_participants.insert(&profile.participant_id);
            total_volume += profile.total_volume_usd;
            
            if profile.total_volume_usd >= self.config.large_transaction_reporting_threshold_usd {
                large_transactions += 1;
            }
            if profile.aml_alerts_count > 0 {
                sar_count += profile.aml_alerts_count;
            }
        }
        
        let report_type = match jurisdiction.as_ref() {
            "US" => ReportType::CTR,
            "EU" => ReportType::EMIR,
            "UK" => ReportType::MiFID,
            "AE" | "SG" | "HK" => ReportType::Custom(jurisdiction.to_string()),
            _ => ReportType::Custom(jurisdiction.to_string()),
        };
        
        RegulatoryReport {
            report_id: Uuid::new_v4().to_string(),
            report_type,
            jurisdiction: jurisdiction.to_string(),
            reporting_period_start: period_start,
            reporting_period_end: period_end,
            generated_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            submitted_at: None,
            status: ReportStatus::Draft,
            transactions,
            summary: ReportSummary {
                total_transactions: 0,
                total_volume_usd: total_volume,
                unique_participants: unique_participants.len() as u32,
                ctr_count,
                sar_count,
                large_transactions,
                cross_border_transactions: 0,
            },
            file_hash: None,
            submission_reference: None,
        }
    }

    async fn process_alert_queue(&self) {
        let alerts = self.alerts.read().await;
        let unresolved: Vec<_> = alerts.iter()
            .filter(|a| !a.resolved && !a.acknowledged)
            .filter(|a| SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() - a.triggered_at > 3600)
            .collect();
        
        if !unresolved.is_empty() {
            warn!("{} unresolved alerts older than 1 hour", unresolved.len());
        }
    }

    pub async fn get_profile(&self, participant_id: &str) -> Option<ParticipantComplianceProfile> {
        self.profiles.read().await.get(participant_id).cloned()
    }

    pub async fn audit_log(
        &self,
        actor: &str,
        action: &str,
        resource_type: &str,
        resource_id: &str,
        result: AuditResult,
        risk_level: RiskLevel,
    ) {
        let entry = AuditLogEntry {
            log_id: Uuid::new_v4().to_string(),
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            actor: actor.to_string(),
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id: resource_id.to_string(),
            details: HashMap::new(),
            ip_address: None,
            user_agent: None,
            result,
            risk_level,
        };
        self.audit_log.write().await.push(entry);
    }

    pub async fn get_audit_log(&self, participant_id: Option<&str>, limit: usize) -> Vec<AuditLogEntry> {
        let log = self.audit_log.read().await;
        log.iter()
            .rev()
            .filter(|e| participant_id.map_or(true, |p| e.resource_id == p || e.actor == p))
            .take(limit)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_compliance_engine() {
        let engine = ComplianceEngine::new(ComplianceConfig::default());
        engine.initialize().await.unwrap();
        
        let profile = engine.register_participant(
            "test_broker".to_string(),
            "Test Broker LLC".to_string(),
            "AE".to_string(),
            Some("549300TEST1234567890".to_string()),
        ).await.unwrap();
        
        assert_eq!(profile.kyc_status, KYCStatus::NotStarted);
        assert_eq!(profile.risk_level, RiskLevel::Low);
        
        let docs = vec![KYCDocument {
            document_id: "doc_1".to_string(),
            document_type: DocumentType::CertificateOfIncorporation,
            file_hash: "hash123".to_string(),
            issued_at: 1609459200,
            expires_at: None,
            issuing_authority: "ADGM".to_string(),
            verified: false,
            verified_at: None,
            verified_by: None,
        }];
        
        engine.submit_kyc_documents("test_broker", docs).await.unwrap();
        engine.review_kyc("test_broker", "compliance_officer", true, "Approved".to_string()).await.unwrap();
        
        let profile = engine.get_profile("test_broker").await.unwrap();
        assert_eq!(profile.kyc_status, KYCStatus::Approved);
    }
}