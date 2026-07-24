use crate::types::DisclosureLevel;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use parking_lot::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

// ==================== LEI Validator (ISO 17442) ====================

#[derive(Debug)]
pub struct LeiValidator;

impl LeiValidator {
    /// Validate LEI per ISO 17442:
    /// - Exactly 20 characters
    /// - Characters 1-4: LOU prefix (must be uppercase alpha)
    /// - Characters 5-18: entity-specific (alphanumeric)
    /// - Characters 19-20: checksum digits (Luhn mod 10)
    pub fn validate(&self, lei: &str) -> bool {
        if lei.len() != 20 { return false; }
        if !lei.chars().all(|c| c.is_ascii_alphanumeric()) { return false; }
        // First 4 chars = LOU code (must be uppercase letters)
        if !lei[..4].chars().all(|c| c.is_ascii_uppercase()) { return false; }
        // Validate LOU prefix against known registrars
        if !self.known_lou(&lei[..4]) { return false; }
        // Validate Luhn checksum (last 2 digits)
        self.luhn_checksum(lei)
    }

    fn known_lou(&self, prefix: &str) -> bool {
        let lous = [
            "5299", "2138", "2549", "3358", "5493", "0970", "9695", "5493",
            "3913", "5662", "5493", "8755", "0970", "9695", "6367", "2138",
            "5493", "3358", "8755", "5299", "9695", "2549", "0970", "5662",
            "3913", "5299", "8755", "9695", "3358", "2549", "2138", "6367",
        ];
        lous.contains(&prefix)
    }

    fn luhn_checksum(&self, lei: &str) -> bool {
        // Luhn algorithm on alphanumeric string
        // Letters A-Z map to values 10-35
        let chars: Vec<char> = lei.chars().collect();
        let mut sum = 0u64;
        let mut double = true;
        for i in (0..chars.len()).rev() {
            let c = chars[i];
            let val = if c.is_ascii_digit() { c as u64 - 48 } else { c as u64 - 55 };
            if double {
                let doubled = val * 2;
                sum += doubled / 10 + doubled % 10;
            } else {
                sum += val;
            }
            double = !double;
        }
        sum % 10 == 0
    }
}

// ==================== Sanctions Database ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanctionedEntity {
    pub name: String,
    pub aliases: Vec<String>,
    pub sanction_type: SanctionType,
    pub jurisdiction: Option<String>,
    pub program: String,
    pub listed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SanctionType {
    Individual,
    Entity,
    Vessel,
    Aircraft,
    Country,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanctionsCheck {
    pub cleared: bool,
    pub matches: Vec<SanctionMatch>,
    pub risk_score: u8,
    pub checked_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanctionMatch {
    pub entity: String,
    pub match_type: MatchType,
    pub confidence: f64,
    pub program: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MatchType {
    Exact,
    Fuzzy,
    Jurisdiction,
    Alias,
}

// ==================== FATF Jurisdiction Classifier ====================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JurisdictionRisk {
    Blacklist,   // FATF High-Risk (call for action)
    Greylist,    // FATF monitored
    High,        // OFAC sanctioned + FATF high
    Medium,      // Limited compliance
    Low,         // Full compliance
}

fn classify_jurisdiction(code: &str) -> (JurisdictionRisk, u8) {
    match code {
        // FATF Blacklist — call for action
        "IR" | "PRK" | "KP" => (JurisdictionRisk::Blacklist, 95),
        // FATF Greylist — monitored
        "MM" | "KY" | "PA" | "BZ" | "UZ" | "PH" | "ZA" | "SS" | "NG" | "YE" => (JurisdictionRisk::Greylist, 70),
        // OFAC Sanctioned
        "CU" | "SY" | "SD" | "BY" | "RU" | "VE" | "CF" | "CD" | "IQ" | "LB" | "LR" | "SO" | "ZW" => (JurisdictionRisk::High, 80),
        // Medium risk
        "AE" | "SA" | "QA" | "OM" | "BH" | "KW" | "JO" | "EG" | "MA" | "TN" | "DZ" | "LY" => (JurisdictionRisk::Medium, 40),
        // Low risk
        "US" | "GB" | "DE" | "FR" | "CH" | "SG" | "HK" | "JP" | "KR" | "AU" | "CA" | "NL" | "SE" | "NO" | "DK" | "FI" | "IE" | "BE" | "AT" | "LU" | "IT" | "ES" | "PT" => (JurisdictionRisk::Low, 10),
        // Unknown default to medium
        _ => (JurisdictionRisk::Medium, 50),
    }
}

// ==================== Soundex for Fuzzy Name Matching ====================

fn soundex(s: &str) -> String {
    let s = s.to_uppercase();
    let chars: Vec<char> = s.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    if chars.is_empty() { return String::new(); }
    let mut result = String::new();
    result.push(chars[0]);
    for &c in &chars[1..] {
        let code = match c {
            'B' | 'F' | 'P' | 'V' => '1',
            'C' | 'G' | 'J' | 'K' | 'Q' | 'S' | 'X' | 'Z' => '2',
            'D' | 'T' => '3',
            'L' => '4',
            'M' | 'N' => '5',
            'R' => '6',
            _ => continue,
        };
        if result.chars().last() != Some(code) && result.len() < 4 {
            result.push(code);
        }
    }
    while result.len() < 4 { result.push('0'); }
    result[..4.min(result.len())].to_string()
}

// ==================== Sanctions Checker ====================

#[derive(Debug)]
pub struct SanctionsChecker {
    sanctioned_countries: Vec<&'static str>,
    sanctioned_entities: Vec<SanctionedEntity>,
    soundex_cache: Mutex<HashMap<String, String>>,
}

impl SanctionsChecker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check(&self, jurisdiction: &str, legal_name: &str) -> SanctionsCheck {
        let checked_at = chrono::Utc::now().timestamp_millis();
        let mut matches = Vec::new();
        let mut risk_score: u8 = 0;

        // 1. Jurisdiction check
        let j = jurisdiction.to_uppercase();
        if self.sanctioned_countries.contains(&j.as_str()) {
            matches.push(SanctionMatch {
                entity: j.clone(),
                match_type: MatchType::Jurisdiction,
                confidence: 1.0,
                program: "OFAC_SANCTIONED_COUNTRY".into(),
            });
            risk_score = risk_score.saturating_add(90);
        }

        // 2. Fuzzy name matching against sanctioned entities
        if !legal_name.is_empty() {
            let name_soundex = self.get_soundex(legal_name);
            for entity in &self.sanctioned_entities {
                // Exact match
                if entity.name.to_uppercase() == legal_name.to_uppercase() {
                    matches.push(SanctionMatch {
                        entity: entity.name.clone(),
                        match_type: MatchType::Exact,
                        confidence: 1.0,
                        program: entity.program.clone(),
                    });
                    risk_score = risk_score.saturating_add(100);
                    continue;
                }
                // Soundex fuzzy match
                let entity_soundex = self.get_soundex(&entity.name);
                if name_soundex == entity_soundex {
                    let confidence = self.name_similarity(legal_name, &entity.name);
                    if confidence > 0.6 {
                        matches.push(SanctionMatch {
                            entity: entity.name.clone(),
                            match_type: MatchType::Fuzzy,
                            confidence,
                            program: entity.program.clone(),
                        });
                        risk_score = risk_score.saturating_add((confidence * 80.0) as u8);
                    }
                }
                // Alias match
                for alias in &entity.aliases {
                    let alias_soundex = self.get_soundex(alias);
                    if name_soundex == alias_soundex {
                        let confidence = self.name_similarity(legal_name, alias);
                        if confidence > 0.6 {
                            matches.push(SanctionMatch {
                                entity: format!("{} (alias: {})", entity.name, alias),
                                match_type: MatchType::Alias,
                                confidence,
                                program: entity.program.clone(),
                            });
                            risk_score = risk_score.saturating_add((confidence * 75.0) as u8);
                        }
                    }
                }
            }
        }

        // Also check jurisdiction risk from FATF
        let (_, j_risk) = classify_jurisdiction(&j);
        risk_score = risk_score.saturating_add(j_risk / 2);

        SanctionsCheck {
            cleared: risk_score < 60,
            matches,
            risk_score: risk_score.min(100),
            checked_at,
        }
    }

    fn get_soundex(&self, name: &str) -> String {
        let mut cache = self.soundex_cache.lock();
        if let Some(s) = cache.get(name) {
            return s.clone();
        }
        let s = soundex(name);
        cache.insert(name.to_string(), s.clone());
        s
    }

    fn name_similarity(&self, a: &str, b: &str) -> f64 {
        let a = a.to_uppercase();
        let b = b.to_uppercase();
        if a == b { return 1.0; }
        // Simple bigram overlap
        let bigrams_a: Vec<&[u8]> = a.as_bytes().windows(2).collect();
        let bigrams_b: Vec<&[u8]> = b.as_bytes().windows(2).collect();
        if bigrams_a.is_empty() || bigrams_b.is_empty() { return 0.0; }
        let overlap = bigrams_a.iter().filter(|ba| bigrams_b.contains(ba)).count();
        let total = bigrams_a.len() + bigrams_b.len();
        if total == 0 { return 0.0; }
        2.0 * overlap as f64 / total as f64
    }
}

impl Default for SanctionsChecker {
    fn default() -> Self {
        Self {
            sanctioned_countries: vec![
                "IR", "PRK", "KP", "CU", "SY", "SD", "BY", "RU",
                "VE", "CF", "CD", "IQ", "LB", "LR", "SO", "SS", "ZW", "YE",
                "MM", "KY",
            ],
            sanctioned_entities: vec![
                // Major OFAC SDN entities (representative subset)
                SanctionedEntity {
                    name: "AL QAEDA".into(),
                    aliases: vec!["AL QAIDA".into(), "THE BASE".into(), "ISLAMIC ARMY".into()],
                    sanction_type: SanctionType::Entity,
                    jurisdiction: None,
                    program: "OFAC_SDN".into(),
                    listed_at: 946684800000,
                },
                SanctionedEntity {
                    name: "TALIBAN".into(),
                    aliases: vec!["ISLAMIC EMIRATE OF AFGHANISTAN".into(), "IEA".into()],
                    sanction_type: SanctionType::Entity,
                    jurisdiction: Some("AF".into()),
                    program: "OFAC_SDN".into(),
                    listed_at: 946684800000,
                },
                SanctionedEntity {
                    name: "ISLAMIC STATE OF IRAQ AND SYRIA".into(),
                    aliases: vec!["ISIS".into(), "ISIL".into(), "DAESH".into(), "ISLAMIC STATE".into()],
                    sanction_type: SanctionType::Entity,
                    jurisdiction: Some("IQ".into()),
                    program: "OFAC_SDN".into(),
                    listed_at: 1388534400000,
                },
                SanctionedEntity {
                    name: "HAMAS".into(),
                    aliases: vec!["HARAKAT AL-MUQAWAMAH AL-ISLAMIYAH".into(), "ISLAMIC RESISTANCE MOVEMENT".into()],
                    sanction_type: SanctionType::Entity,
                    jurisdiction: Some("PS".into()),
                    program: "OFAC_SDN".into(),
                    listed_at: 946684800000,
                },
                SanctionedEntity {
                    name: "HEZBOLLAH".into(),
                    aliases: vec!["HIZBALLAH".into(), "PARTY OF GOD".into(), "ISLAMIC JIHAD ORGANIZATION".into()],
                    sanction_type: SanctionType::Entity,
                    jurisdiction: Some("LB".into()),
                    program: "OFAC_SDN".into(),
                    listed_at: 946684800000,
                },
                SanctionedEntity {
                    name: "KOREAN PEOPLE'S ARMY".into(),
                    aliases: vec!["KPA".into(), "NORTH KOREAN ARMY".into()],
                    sanction_type: SanctionType::Entity,
                    jurisdiction: Some("KP".into()),
                    program: "OFAC_SDN".into(),
                    listed_at: 946684800000,
                },
                SanctionedEntity {
                    name: "IRANIAN REVOLUTIONARY GUARD CORPS".into(),
                    aliases: vec!["IRGC".into(), "PASDARAN".into(), "SEPAH".into()],
                    sanction_type: SanctionType::Entity,
                    jurisdiction: Some("IR".into()),
                    program: "OFAC_SDN".into(),
                    listed_at: 1262304000000,
                },
                SanctionedEntity {
                    name: "RUSSIAN FEDERATION CENTRAL BANK".into(),
                    aliases: vec!["BANK OF RUSSIA".into(), "ЦБ РФ".into()],
                    sanction_type: SanctionType::Entity,
                    jurisdiction: Some("RU".into()),
                    program: "OFAC_UKRAINE_RELATED".into(),
                    listed_at: 1646198400000,
                },
                SanctionedEntity {
                    name: "GRU".into(),
                    aliases: vec!["MAIN INTELLIGENCE DIRECTORATE".into(), "ГРУ".into(), "GU".into()],
                    sanction_type: SanctionType::Entity,
                    jurisdiction: Some("RU".into()),
                    program: "OFAC_UKRAINE_RELATED".into(),
                    listed_at: 1643691600000,
                },
                SanctionedEntity {
                    name: "FSB".into(),
                    aliases: vec!["FEDERAL SECURITY SERVICE".into(), "ФСБ".into(), "KGB".into()],
                    sanction_type: SanctionType::Entity,
                    jurisdiction: Some("RU".into()),
                    program: "OFAC_UKRAINE_RELATED".into(),
                    listed_at: 1643691600000,
                },
                // Major sanctioned individuals
                SanctionedEntity {
                    name: "VLADIMIR PUTIN".into(),
                    aliases: vec!["VLADIMIR VLADIMIROVICH PUTIN".into(), "PUTIN V V".into()],
                    sanction_type: SanctionType::Individual,
                    jurisdiction: Some("RU".into()),
                    program: "OFAC_UKRAINE_RELATED".into(),
                    listed_at: 1646198400000,
                },
                SanctionedEntity {
                    name: "KIM JONG UN".into(),
                    aliases: vec!["KIM JONG-EUN".into(), "KIM JONG UN".into(), "KIM JONG WUN".into()],
                    sanction_type: SanctionType::Individual,
                    jurisdiction: Some("KP".into()),
                    program: "OFAC_SDN".into(),
                    listed_at: 1325376000000,
                },
                SanctionedEntity {
                    name: "BASHAR AL ASSAD".into(),
                    aliases: vec!["BASHAR HAFEZ AL-ASSAD".into(), "BASHAR AL-ASSAD".into(), "DR BASHAR AL ASSAD".into()],
                    sanction_type: SanctionType::Individual,
                    jurisdiction: Some("SY".into()),
                    program: "OFAC_SDN".into(),
                    listed_at: 1341100800000,
                },
            ],
            soundex_cache: Mutex::new(HashMap::new()),
        }
    }
}

// ==================== AML Transaction Monitor ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmlCheck {
    pub passed: bool,
    pub risk_score: u8,
    pub risk_level: String,
    pub flags: Vec<String>,
    pub sar_required: bool,
    pub checked_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SarRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub transaction_ids: Vec<Uuid>,
    pub risk_score: u8,
    pub flags: Vec<String>,
    pub narrative: String,
    pub filed_at: i64,
    pub filed_to_regulator: bool,
}

#[derive(Debug, Clone)]
struct TransactionRecord {
    id: Uuid,
    #[allow(dead_code)]
    tenant_id: Uuid,
    side: String,
    quantity: f64,
    price: f64,
    timestamp: i64,
    #[allow(dead_code)]
    jurisdiction: String,
}

#[derive(Debug)]
pub struct AmlMonitor {
    tx_history: Mutex<HashMap<Uuid, VecDeque<TransactionRecord>>>,
    sar_log: Mutex<Vec<SarRecord>>,
    sar_counter: AtomicU64,
}

impl AmlMonitor {
    pub fn new() -> Self {
        Self {
            tx_history: Mutex::new(HashMap::new()),
            sar_log: Mutex::new(Vec::new()),
            sar_counter: AtomicU64::new(0),
        }
    }

    pub fn check_transaction(
        &self,
        tenant_id: Uuid,
        side: &str,
        quantity: f64,
        price: f64,
        jurisdiction: &str,
        sanctions_check: &SanctionsCheck,
    ) -> AmlCheck {
        let now = chrono::Utc::now().timestamp_millis();
        let mut flags: Vec<String> = Vec::new();
        let mut risk_score: u8 = 0;

        // 1. Carry over sanctions risk
        risk_score = risk_score.saturating_add(sanctions_check.risk_score / 2);

        // 2. Record this transaction
        let tx_record = TransactionRecord {
            id: Uuid::new_v4(),
            tenant_id,
            side: side.to_string(),
            quantity,
            price,
            timestamp: now,
            jurisdiction: jurisdiction.to_string(),
        };

        let recent_txns = {
            let mut history = self.tx_history.lock();
            let txns = history.entry(tenant_id).or_insert_with(|| VecDeque::with_capacity(100));
            txns.push_back(tx_record);
            // Keep last 100 transactions
            while txns.len() > 100 { txns.pop_front(); }
            txns.iter().map(|t| t.clone()).collect::<Vec<_>>()
        };

        // 3. Structuring detection (Smurfing) — multiple txns just below threshold
        let threshold: f64 = 10_000.0;
        let near_threshold_txns: Vec<_> = recent_txns.iter()
            .filter(|t| {
                let val = t.quantity * t.price;
                val > threshold * 0.8 && val < threshold
            })
            .collect();
        if near_threshold_txns.len() >= 3 {
            flags.push(format!("STRUCTURING_DETECTED: {} transactions near ${} threshold", near_threshold_txns.len(), threshold));
            risk_score = risk_score.saturating_add(40);
        }

        // 4. Velocity check — rapid transactions in short window
        let window_ms = 10_000i64; // 10 seconds
        let rapid_txns: Vec<_> = recent_txns.iter()
            .filter(|t| now - t.timestamp < window_ms)
            .collect();
        if rapid_txns.len() >= 5 {
            flags.push(format!("HIGH_VELOCITY: {} transactions in 10 seconds", rapid_txns.len()));
            risk_score = risk_score.saturating_add(35);
        } else if rapid_txns.len() >= 3 {
            flags.push(format!("ELEVATED_VELOCITY: {} transactions in 10 seconds", rapid_txns.len()));
            risk_score = risk_score.saturating_add(20);
        }

        // 5. Round number detection (money laundering pattern)
        let total = quantity * price;
        if total >= 1000.0 && total % 1000.0 < 0.01 {
            flags.push("ROUND_NUMBER_TRANSACTION".into());
            risk_score = risk_score.saturating_add(10);
        }

        // 6. Rapid buy/sell oscillation (wash trading pattern)
        let recent_sides: Vec<&str> = rapid_txns.iter().map(|t| t.side.as_str()).collect();
        if recent_sides.len() >= 4 {
            let alternating = recent_sides.windows(2).all(|w| w[0] != w[1]);
            if alternating {
                flags.push("RAPID_BUY_SELL_OSCILLATION".into());
                risk_score = risk_score.saturating_add(30);
            }
        }

        // 7. High value transaction
        if total > 100_000.0 {
            flags.push(format!("HIGH_VALUE: ${}", total));
            risk_score = risk_score.saturating_add(25);
        } else if total > 50_000.0 {
            flags.push(format!("ELEVATED_VALUE: ${}", total));
            risk_score = risk_score.saturating_add(15);
        }

        // 8. Jurisdiction risk from AML perspective
        let (_, j_risk) = classify_jurisdiction(jurisdiction);
        if j_risk >= 80 {
            flags.push(format!("HIGH_RISK_JURISDICTION: {}", jurisdiction));
            risk_score = risk_score.saturating_add(j_risk / 3);
        }

        let risk_score = risk_score.min(100);
        let risk_level = if risk_score >= 80 { "CRITICAL" }
            else if risk_score >= 60 { "HIGH" }
            else if risk_score >= 35 { "MEDIUM" }
            else { "LOW" };

        let passed = risk_score < 60;
        let sar_required = risk_score >= 80;

        // Generate SAR if critical
        if sar_required {
            let sar = SarRecord {
                id: Uuid::new_v4(),
                tenant_id,
                transaction_ids: recent_txns.iter().map(|t| t.id).collect(),
                risk_score,
                flags: flags.clone(),
                narrative: format!("AUTO_SAR: Risk score {} - {}", risk_score, flags.join("; ")),
                filed_at: chrono::Utc::now().timestamp_millis(),
                filed_to_regulator: false,
            };
            let mut log = self.sar_log.lock();
            log.push(sar);
            self.sar_counter.fetch_add(1, Ordering::Relaxed);
            warn!(target: "compliance", tenant=%tenant_id, risk=risk_score, "SAR FILED");
        }

        AmlCheck { passed, risk_score, risk_level: risk_level.into(), flags, sar_required, checked_at: now }
    }

    pub fn get_sar_log(&self, limit: usize) -> Vec<SarRecord> {
        let log = self.sar_log.lock();
        log.iter().rev().take(limit).cloned().collect()
    }

    pub fn total_sars(&self) -> u64 {
        self.sar_counter.load(Ordering::Relaxed)
    }
}

// ==================== Compliance Gateway ====================

#[derive(Debug)]
pub struct ComplianceGateway {
    pub lei_validator: LeiValidator,
    pub sanctions_checker: SanctionsChecker,
    pub aml_monitor: AmlMonitor,
}

impl ComplianceGateway {
    pub fn new() -> Self {
        Self {
            lei_validator: LeiValidator,
            sanctions_checker: SanctionsChecker::new(),
            aml_monitor: AmlMonitor::new(),
        }
    }

    pub fn onboard_entity(
        &self,
        tenant_id: Uuid,
        legal_name: &str,
        lei: &str,
        jurisdiction: &str,
    ) -> Result<EntityProfile, String> {
        // 1. Validate LEI
        if !self.lei_validator.validate(lei) {
            return Err("Invalid LEI format or checksum".into());
        }

        // 2. Sanctions check
        let sanctions = self.sanctions_checker.check(jurisdiction, legal_name);
        if !sanctions.cleared {
            let reasons: Vec<String> = sanctions.matches.iter()
                .map(|m| format!("{} ({:?}, confidence {:.0}%)", m.entity, m.match_type, m.confidence * 100.0))
                .collect();
            warn!(target: "compliance", tenant=%tenant_id, matches=%reasons.join(", "), "SANCTIONS BLOCKED");
            return Err(format!("Sanctions check failed: {}", reasons.join("; ")));
        }

        // 3. Classify jurisdiction risk
        let (j_risk_level, j_risk_score) = classify_jurisdiction(&jurisdiction.to_uppercase());

        let profile = EntityProfile {
            tenant_id,
            legal_name: legal_name.to_string(),
            lei: lei.to_string(),
            jurisdiction: jurisdiction.to_uppercase(),
            disclosure_level: if j_risk_score < 20 { DisclosureLevel::Verified } else { DisclosureLevel::Public },
            verified_at: chrono::Utc::now().timestamp_millis(),
            risk_score: j_risk_score,
        };

        info!(
            target: "compliance",
            tenant_id = %tenant_id,
            jurisdiction = %jurisdiction.to_uppercase(),
            risk = j_risk_score,
            risk_level = ?j_risk_level,
            "Entity onboarded"
        );

        Ok(profile)
    }

    pub fn check_transaction_aml(
        &self,
        tenant_id: Uuid,
        side: &str,
        quantity: f64,
        price: f64,
        jurisdiction: &str,
        legal_name: &str,
    ) -> AmlCheck {
        let sanctions = self.sanctions_checker.check(jurisdiction, legal_name);
        self.aml_monitor.check_transaction(tenant_id, side, quantity, price, jurisdiction, &sanctions)
    }

    pub fn upgrade_disclosure(
        &self,
        current: &DisclosureLevel,
        verified_lei: bool,
        has_nda: bool,
        is_sovereign_fund: bool,
    ) -> Option<DisclosureLevel> {
        match current {
            DisclosureLevel::Public if verified_lei => Some(DisclosureLevel::Verified),
            DisclosureLevel::Verified if has_nda => Some(DisclosureLevel::Institutional),
            DisclosureLevel::Institutional if is_sovereign_fund => Some(DisclosureLevel::Sovereign),
            _ => None,
        }
    }

    pub fn get_sars(&self, limit: usize) -> Vec<SarRecord> {
        self.aml_monitor.get_sar_log(limit)
    }

    pub fn total_sars(&self) -> u64 {
        self.aml_monitor.total_sars()
    }
}

// ==================== Public API ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityProfile {
    pub tenant_id: Uuid,
    pub legal_name: String,
    pub lei: String,
    pub jurisdiction: String,
    pub disclosure_level: DisclosureLevel,
    pub verified_at: i64,
    pub risk_score: u8,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KycStatus {
    pub tenant_id: Uuid,
    pub lei_verified: bool,
    pub sanctions_cleared: bool,
    pub sanctions_detail: SanctionsCheck,
    pub disclosure_level: DisclosureLevel,
    pub verified_at: Option<i64>,
}

#[allow(dead_code)]
pub fn filter_response_by_disclosure<T>(
    data: T,
    _disclosure: &DisclosureLevel,
) -> Result<T, String> {
    Ok(data)
}
