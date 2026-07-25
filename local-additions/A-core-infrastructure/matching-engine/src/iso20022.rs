use crate::pipeline::TradePayload;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

pub fn build_camt_054(trades: &[TradePayload]) -> String {
    if trades.is_empty() {
        return String::new();
    }
    let now = chrono::Utc::now();
    let msg_id = format!("TB{:016x}", now.timestamp_millis() as u64);

    let mut xml = String::with_capacity(4096);
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    xml.push_str("<Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:camt.054.001.09\">");
    xml.push_str("<BkToCstmrDbtCdtNtfctn>");
    xml.push_str(&format!(
        "<GrpHdr><MsgId>{}</MsgId><CreDtTm>{}</CreDtTm></GrpHdr>",
        msg_id,
        now.format("%Y-%m-%dT%H:%M:%S%.3fZ")
    ));

    for trade in trades {
        let trade_id = format!("{:016x}", trade.trade_id);
        xml.push_str("<Ntfctn><Id>");
        xml.push_str(&trade_id);
        xml.push_str("</Id><Ntry>");
        xml.push_str(&format!("<Amt Ccy=\"USD\">{:.2}</Amt>", trade.total as f64 / 1_000_000.0));
        xml.push_str("<CdtDbtInd>DBIT</CdtDbtInd>");
        xml.push_str("<BkTxCd><Prtry><Cd>TRADE</Cd></Prtry></BkTxCd>");
        xml.push_str("<NtryDtls><TxDtls>");
        xml.push_str(&format!("<Refs><TxId>{}</TxId></Refs>", trade_id));
        xml.push_str(&format!(
            "<AmtDtls><InstdAmt Ccy=\"USD\">{:.2}</InstdAmt></AmtDtls>",
            trade.quantity as f64 / 1_000_000.0
        ));
        xml.push_str(&format!("<Purp><Prtry>{}</Prtry></Purp>", trade.pair_str()));
        xml.push_str(&format!(
            "<RltdPties><Buyr><Id><PrtryId><Id>{:016x}</Id></PrtryId></Id></Buyr>",
            trade.buy_user_id
        ));
        xml.push_str(&format!(
            "<Sellr><Id><PrtryId><Id>{:016x}</Id></PrtryId></Id></Sellr>",
            trade.sell_user_id
        ));
        xml.push_str("</RltdPties>");
        xml.push_str("<Sts><Rsn>SETT</Rsn></Sts>");
        xml.push_str("</TxDtls></NtryDtls>");
        xml.push_str("</Ntry></Ntfctn>");
    }

    xml.push_str("</BkToCstmrDbtCdtNtfctn>");
    xml.push_str("</Document>");
    xml
}

pub struct Iso20022Queue {
    dir: PathBuf,
    seq: AtomicU64,
}

impl Iso20022Queue {
    pub fn new(dir: &str) -> Result<Self, String> {
        let path = PathBuf::from(dir);
        fs::create_dir_all(&path).map_err(|e| format!("iso20022 dir: {e}"))?;
        Ok(Self {
            dir: path,
            seq: AtomicU64::new(0),
        })
    }

    pub fn push(&self, report: &Iso20022Report) -> Result<(), String> {
        if report.xml_content.is_empty() {
            return Ok(());
        }
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        let filename = format!("{:020}_{}.xml", seq, report.msg_type);
        let path = self.dir.join(&filename);
        fs::write(&path, &report.xml_content)
            .map_err(|e| format!("iso20022 write {filename}: {e}"))?;
        Ok(())
    }

    pub fn list_reports(&self, limit: usize) -> Result<Vec<Iso20022ReportSummary>, String> {
        let mut entries: Vec<_> = fs::read_dir(&self.dir)
            .map_err(|e| format!("read iso20022 dir: {e}"))?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "xml").unwrap_or(false))
            .collect();
        entries.sort_by_key(|e| e.file_name());
        let limit = limit.min(entries.len());
        let mut reports = Vec::with_capacity(limit);
        for entry in entries.into_iter().rev().take(limit) {
            let meta = entry.metadata().ok();
            reports.push(Iso20022ReportSummary {
                filename: entry.file_name().to_string_lossy().to_string(),
                size_bytes: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                modified_at: meta.and_then(|m| m.modified().ok().map(|t| {
                    t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64
                })).unwrap_or(0),
            });
        }
        Ok(reports)
    }

    pub fn get_report(&self, filename: &str) -> Result<String, String> {
        let path = self.dir.join(filename);
        if !path.starts_with(&self.dir) {
            return Err("path traversal".to_string());
        }
        fs::read_to_string(&path).map_err(|e| format!("read iso20022: {e}"))
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Iso20022ReportSummary {
    pub filename: String,
    pub size_bytes: u64,
    pub modified_at: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Iso20022Report {
    pub msg_type: String,
    pub xml_content: String,
    pub trade_count: usize,
    pub generated_at: i64,
}

pub fn build_iso_20022_report(trades: &[TradePayload]) -> Iso20022Report {
    let xml = build_camt_054(trades);
    Iso20022Report {
        msg_type: "camt.054.001.09".to_string(),
        xml_content: xml,
        trade_count: trades.len(),
        generated_at: chrono::Utc::now().timestamp_millis(),
    }
}
