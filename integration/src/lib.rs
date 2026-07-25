use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tracing::{error, info};
use uuid::Uuid;
pub type Address = [u8; 20];
pub type H256 = [u8; 32];
pub type U256 = u128;
#[derive(Debug, Clone)]
pub enum IntegrationError {
    ComponentNotFound(String), ComponentAlreadyRegistered(String), HealthCheckFailed(String),
    EventBusFull, Serialization(String), Timeout(String), ConfigNotFound(String), ServiceUnavailable(String),
}
impl std::fmt::Display for IntegrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self { Self::ComponentNotFound(s) => write!(f,"Not found: {}",s), Self::ComponentAlreadyRegistered(s) => write!(f,"Exists: {}",s), Self::HealthCheckFailed(s) => write!(f,"Health: {}",s), Self::EventBusFull => write!(f,"Bus full"), Self::Serialization(s) => write!(f,"Ser: {}",s), Self::Timeout(s) => write!(f,"Timeout: {}",s), Self::ConfigNotFound(s) => write!(f,"No config: {}",s), Self::ServiceUnavailable(s) => write!(f,"Unavail: {}",s), }
    }
}
impl std::error::Error for IntegrationError {}
impl From<serde_json::Error> for IntegrationError { fn from(e: serde_json::Error) -> Self { Self::Serialization(e.to_string()) } }
pub type Result<T> = std::result::Result<T, IntegrationError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemEvent {
    ComponentStarted{name:String,instance_id:Uuid}, ComponentStopped{name:String,instance_id:Uuid},
    HealthChanged{component:String,healthy:bool,error:Option<String>}, CircuitBreakerTriggered{component:String,reason:String},
    MetricsUpdated{component:String,tps:f64,latency_p99:Duration}, Error{component:String,message:String,severity:ErrorSeverity},
    Custom{event_type:String,data:String},
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)] pub enum ErrorSeverity { Low, Medium, High, Critical }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub component: String, pub instance_id: Uuid, pub healthy: bool, pub last_heartbeat: u64,
    pub uptime: Duration, pub error_count: u64, pub latency_p50: Duration, pub latency_p90: Duration,
    pub latency_p99: Duration, pub memory_used_mb: u64, pub cpu_usage_pct: f64,
}

#[derive(Clone)]
pub struct EventBus { name: String, tx: broadcast::Sender<SystemEvent>, rx_count: Arc<RwLock<usize>>, event_count: Arc<RwLock<u64>> }
impl EventBus {
    pub fn new(name: &str, capacity: usize) -> Self { let (tx,_)=broadcast::channel(capacity); Self{name:name.into(),tx,rx_count:Arc::new(RwLock::new(0)),event_count:Arc::new(RwLock::new(0))} }
    pub fn publish(&self, event: SystemEvent) -> Result<()> { match self.tx.send(event) { Ok(_) => { *self.event_count.write()+=1; Ok(()) } Err(_)=>Err(IntegrationError::EventBusFull) } }
    pub fn subscribe(&self) -> broadcast::Receiver<SystemEvent> { *self.rx_count.write()+=1; self.tx.subscribe() }
    pub fn subscriber_count(&self) -> usize { self.tx.receiver_count() }
    pub fn event_count(&self) -> u64 { *self.event_count.read() }
    pub fn name(&self) -> &str { &self.name }
}

#[derive(Debug, Clone)]
pub struct ComponentMetadata { pub name: String, pub version: String, pub description: String, pub dependencies: Vec<String>, pub capabilities: Vec<String> }

#[async_trait]
pub trait ComponentLifecycle: Send + Sync {
    fn name(&self) -> &str; fn metadata(&self) -> ComponentMetadata;
    async fn start(&self) -> Result<()>; async fn stop(&self) -> Result<()>; async fn health_check(&self) -> Result<HealthReport>;
    async fn restart(&self) -> Result<()> { self.stop().await?; tokio::time::sleep(Duration::from_millis(100)).await; self.start().await }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct HealthAggregator { reports: Arc<DashMap<String,HealthReport>>, bus: Option<Arc<EventBus>>, interval: Duration, threshold: u32 }
impl HealthAggregator {
    pub fn new(interval: Duration, threshold: u32) -> Self { Self{reports:Arc::new(DashMap::new()),bus:None,interval,threshold} }
    pub fn with_bus(mut self, bus: Arc<EventBus>) -> Self { self.bus=Some(bus); self }
    pub fn update(&self, report: HealthReport) {
        let was = self.reports.get(&report.component).map(|r| r.healthy).unwrap_or(true);
        self.reports.insert(report.component.clone(), report.clone());
        if was != report.healthy { if let Some(ref b)=self.bus { let _=b.publish(SystemEvent::HealthChanged{component:report.component.clone(),healthy:report.healthy,error:None}); } }
    }
    pub fn get(&self, c: &str) -> Option<HealthReport> { self.reports.get(c).map(|r| r.clone()) }
    pub fn all(&self) -> Vec<HealthReport> { self.reports.iter().map(|r| r.clone()).collect() }
    pub fn is_healthy(&self) -> bool { self.reports.iter().all(|r| r.healthy) }
    pub fn unhealthy(&self) -> Vec<String> { self.reports.iter().filter(|r|!r.healthy).map(|r| r.component.clone()).collect() }
    pub fn count(&self) -> usize { self.reports.len() }
    pub fn monitor(self: Arc<Self>, components: Vec<Arc<dyn ComponentLifecycle>>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move { let mut tick = tokio::time::interval(self.interval); loop { tick.tick().await; for c in &components { if let Ok(r)=c.health_check().await { self.update(r); } } } })
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct MetricsCollector { component: String, gauges: Arc<DashMap<String,f64>>, counters: Arc<DashMap<String,u64>>, hists: Arc<DashMap<String,Vec<f64>>>, bus: Option<Arc<EventBus>>, max_samples: usize }
impl MetricsCollector {
    pub fn new(component: &str) -> Self { Self{component:component.into(),gauges:Arc::new(DashMap::new()),counters:Arc::new(DashMap::new()),hists:Arc::new(DashMap::new()),bus:None,max_samples:1000} }
    pub fn with_bus(mut self, bus: Arc<EventBus>) -> Self { self.bus=Some(bus); self }
    pub fn gauge(&self, name: &str, val: f64) { self.gauges.insert(name.into(), val); }
    pub fn inc(&self, name: &str, delta: u64) { *self.counters.entry(name.into()).or_insert(0) += delta; }
    pub fn observe(&self, name: &str, val: f64) { let mut h=self.hists.entry(name.into()).or_insert_with(Vec::new); h.push(val); if h.len()>self.max_samples { h.remove(0); } }
    pub fn get_gauge(&self, name: &str) -> Option<f64> { self.gauges.get(name).map(|v|*v) }
    pub fn get_counter(&self, name: &str) -> u64 { self.counters.get(name).map(|v|*v).unwrap_or(0) }
    pub fn percentile(&self, name: &str, pct: f64) -> Option<f64> { self.hists.get(name).and_then(|h|{if h.is_empty(){return None}let mut s=h.clone();s.sort_by(|a,b|a.partial_cmp(b).unwrap());s.get(((pct/100.0)*(s.len()-1)as f64)as usize).copied()}) }
    pub fn snapshot(&self) -> HashMap<String,f64> { let mut m=HashMap::new(); for e in self.gauges.iter(){m.insert(e.key().clone(),*e.value());} for e in self.counters.iter(){m.insert(e.key().clone(),*e.value()as f64);} for e in self.hists.iter(){if let Some(p50)=self.percentile(e.key(),50.0){m.insert(format!("{}_p50",e.key()),p50);}if let Some(p99)=self.percentile(e.key(),99.0){m.insert(format!("{}_p99",e.key()),p99);}} m }
    pub fn record_tps(&self, tps: f64) { self.gauge("tps",tps); self.observe("tps",tps); }
    pub fn record_latency(&self, lat: Duration) { let ms=lat.as_secs_f64()*1000.0; self.gauge("latency_ms",ms); self.observe("latency_ms",ms); }
}

#[derive(Debug, Clone, Serialize, Deserialize)] pub struct ConfigEntry { pub key: String, pub value: String, pub updated_at: u64, pub version: u32 }
pub struct ConfigManager { configs: Arc<DashMap<String,ConfigEntry>>, listeners: Arc<RwLock<Vec<mpsc::UnboundedSender<(String,String)>>>>, max_ver: Arc<RwLock<u32>> }
impl ConfigManager {
    pub fn new() -> Self { Self{configs:Arc::new(DashMap::new()),listeners:Arc::new(RwLock::new(Vec::new())),max_ver:Arc::new(RwLock::new(0))} }
    pub fn set(&self, key: &str, val: &str) -> Result<()> {
        let now=Utc::now().timestamp() as u64; let mut v=self.max_ver.write(); *v+=1;
        let e=ConfigEntry{key:key.into(),value:val.into(),updated_at:now,version:*v}; self.configs.insert(key.into(),e);
        for l in self.listeners.read().iter() { let _=l.send((key.into(),val.into())); } Ok(())
    }
    pub fn get(&self, key: &str) -> Result<String> { self.configs.get(key).map(|e|e.value.clone()).ok_or_else(||IntegrationError::ConfigNotFound(key.into())) }
    pub fn get_int(&self, key: &str) -> Result<i64> { self.get(key)?.parse().map_err(|_|IntegrationError::ConfigNotFound(format!("parse int {}",key))) }
    pub fn get_bool(&self, key: &str) -> Result<bool> { match self.get(key)?.to_lowercase().as_str() { "true"|"1"|"yes"=>Ok(true), "false"|"0"|"no"=>Ok(false), _=>Err(IntegrationError::ConfigNotFound(format!("parse bool {}",key))) } }
    pub fn watch(&self) -> mpsc::UnboundedReceiver<(String,String)> { let(tx,rx)=mpsc::unbounded_channel(); self.listeners.write().push(tx); rx }
    pub fn all(&self) -> Vec<ConfigEntry> { self.configs.iter().map(|e|e.clone()).collect() }
    pub fn has(&self, key: &str) -> bool { self.configs.contains_key(key) }
}

#[derive(Debug, Clone)] pub struct ServiceInstance { pub id: Uuid, pub name: String, pub version: String, pub endpoints: Vec<String>, pub healthy: bool, pub last_seen: u64, pub load: f64 }
pub struct ServiceMesh { services: Arc<DashMap<String,Vec<ServiceInstance>>>, bus: Option<Arc<EventBus>> }
impl ServiceMesh {
    pub fn new() -> Self { Self{services:Arc::new(DashMap::new()),bus:None} }
    pub fn with_bus(mut self, bus: Arc<EventBus>) -> Self { self.bus=Some(bus); self }
    pub fn register(&self, name: &str, ver: &str, endpoints: Vec<String>) -> ServiceInstance {
        let inst=ServiceInstance{id:Uuid::new_v4(),name:name.into(),version:ver.into(),endpoints,healthy:true,last_seen:Utc::now().timestamp() as u64,load:0.0};
        self.services.entry(name.into()).or_insert_with(Vec::new).push(inst.clone()); inst
    }
    pub fn deregister(&self, name: &str, id: Uuid) -> Result<()> {
        if let Some(mut v)=self.services.get_mut(name){v.retain(|i|i.id!=id); Ok(())}else{Err(IntegrationError::ComponentNotFound(name.into()))}
    }
    pub fn discover(&self, name: &str) -> Result<Vec<ServiceInstance>> { self.services.get(name).map(|v| v.iter().filter(|i|i.healthy).cloned().collect()).ok_or_else(||IntegrationError::ComponentNotFound(name.into())) }
    pub fn discover_one(&self, name: &str) -> Result<ServiceInstance> { let v=self.discover(name)?; v.into_iter().min_by_key(|i|(i.load*1000.0)as u64).ok_or_else(||IntegrationError::ServiceUnavailable(name.into())) }
    pub fn heartbeat(&self, name: &str, id: Uuid, load: f64) -> Result<()> {
        if let Some(mut v)=self.services.get_mut(name){if let Some(i)=v.iter_mut().find(|x|x.id==id){i.last_seen=Utc::now().timestamp() as u64;i.load=load;return Ok(())}} Err(IntegrationError::ComponentNotFound(name.into()))
    }
    pub fn mark_unhealthy(&self, name: &str, id: Uuid) -> Result<()> {
        if let Some(mut v)=self.services.get_mut(name){if let Some(i)=v.iter_mut().find(|x|x.id==id){i.healthy=false;return Ok(())}} Err(IntegrationError::ComponentNotFound(name.into()))
    }
    pub fn all_services(&self) -> Vec<String> { self.services.iter().map(|e|e.key().clone()).collect() }
    pub fn count(&self) -> usize { self.services.len() }
    pub fn total_instances(&self) -> usize { self.services.iter().map(|e|e.value().len()).sum() }
}

pub struct IntegrationEngine {
    pub bus: Arc<EventBus>, pub health: Arc<HealthAggregator>, pub metrics: Arc<DashMap<String,Arc<MetricsCollector>>>,
    pub config: Arc<ConfigManager>, pub mesh: Arc<ServiceMesh>, components: Arc<RwLock<Vec<Arc<dyn ComponentLifecycle>>>>,
}
impl IntegrationEngine {
    pub fn new() -> Self { Self{bus:Arc::new(EventBus::new("system",10000)),health:Arc::new(HealthAggregator::new(Duration::from_secs(5),3)),metrics:Arc::new(DashMap::new()),config:Arc::new(ConfigManager::new()),mesh:Arc::new(ServiceMesh::new()),components:Arc::new(RwLock::new(Vec::new()))} }
    pub fn register(&self, component: Arc<dyn ComponentLifecycle>) -> Result<()> {
        let name=component.name().to_string(); let mut c=self.components.write();
        if c.iter().any(|x|x.name()==name){return Err(IntegrationError::ComponentAlreadyRegistered(name));}
        c.push(component); self.metrics.insert(name.clone(),Arc::new(MetricsCollector::new(&name))); info!(name,"Registered"); Ok(())
    }
    pub fn get_metrics(&self, c: &str) -> Option<Arc<MetricsCollector>> { self.metrics.get(c).map(|m|m.clone()) }
    pub fn start_all(&self) -> Vec<tokio::task::JoinHandle<()>> { self.components.read().iter().map(|c|{let cc=c.clone();tokio::spawn(async move{if let Err(e)=cc.start().await{error!(name=cc.name(),%e,"Start fail")}})}).collect() }
    pub async fn stop_all(&self) { for c in self.components.read().iter() { if let Err(e)=c.stop().await{error!(name=c.name(),%e,"Stop fail")} } }
    pub fn component_count(&self) -> usize { self.components.read().len() }
    pub fn component_names(&self) -> Vec<String> { self.components.read().iter().map(|c|c.name().to_string()).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestComp{name:String,ok:bool}
    #[async_trait] impl ComponentLifecycle for TestComp {
        fn name(&self)->&str{&self.name}
        fn metadata(&self)->ComponentMetadata{ComponentMetadata{name:self.name.clone(),version:"1".into(),description:"test".into(),dependencies:vec![],capabilities:vec!["test".into()]}}
        async fn start(&self)->Result<()>{Ok(())} async fn stop(&self)->Result<()>{Ok(())}
        async fn health_check(&self)->Result<HealthReport>{Ok(HealthReport{component:self.name.clone(),instance_id:Uuid::new_v4(),healthy:self.ok,last_heartbeat:Utc::now().timestamp()as u64,uptime:Duration::from_secs(100),error_count:0,latency_p50:Duration::from_micros(10),latency_p90:Duration::from_micros(50),latency_p99:Duration::from_micros(100),memory_used_mb:128,cpu_usage_pct:25.0})}
    }
    #[test] fn test_bus() { let b=EventBus::new("t",100); let mut rx=b.subscribe(); b.publish(SystemEvent::ComponentStarted{name:"x".into(),instance_id:Uuid::new_v4()}).unwrap(); assert!(rx.try_recv().is_ok()); }
    #[test] fn test_bus_count() { let b=EventBus::new("t",100); let _=b.subscribe(); let _=b.subscribe(); assert_eq!(b.subscriber_count(),2); }
    #[test] fn test_health() { let h=HealthAggregator::new(Duration::from_secs(5),3); h.update(HealthReport{component:"e".into(),instance_id:Uuid::new_v4(),healthy:true,last_heartbeat:0,uptime:Duration::from_secs(1),error_count:0,latency_p50:Duration::from_secs(0),latency_p90:Duration::from_secs(0),latency_p99:Duration::from_secs(0),memory_used_mb:0,cpu_usage_pct:0.0}); assert!(h.is_healthy()); }
    #[test] fn test_health_unhealthy() { let h=HealthAggregator::new(Duration::from_secs(5),3); h.update(HealthReport{component:"e".into(),instance_id:Uuid::new_v4(),healthy:false,last_heartbeat:0,uptime:Duration::from_secs(1),error_count:5,latency_p50:Duration::from_secs(0),latency_p90:Duration::from_secs(0),latency_p99:Duration::from_secs(0),memory_used_mb:0,cpu_usage_pct:0.0}); assert!(!h.is_healthy()); assert_eq!(h.unhealthy().len(),1); }
    #[test] fn test_metrics() { let m=MetricsCollector::new("t"); m.gauge("tps",1000.0); assert_eq!(m.get_gauge("tps"),Some(1000.0)); m.inc("orders",5); assert_eq!(m.get_counter("orders"),5); for i in 1..=100 { m.observe("lat",i as f64); } assert!(m.percentile("lat",50.0).unwrap()>0.0); }
    #[test] fn test_config() { let c=ConfigManager::new(); c.set("key","val").unwrap(); assert_eq!(c.get("key").unwrap(),"val"); assert!(c.has("key")); c.set("port","8080").unwrap(); assert_eq!(c.get_int("port").unwrap(),8080); c.set("on","true").unwrap(); assert!(c.get_bool("on").unwrap()); }
    #[test] fn test_config_watch() { let c=ConfigManager::new(); let mut rx=c.watch(); c.set("k","v").unwrap(); assert_eq!(rx.try_recv().unwrap(),("k".into(),"v".into())); }
    #[test] fn test_mesh() { let m=ServiceMesh::new(); let i=m.register("e","1",vec!["http://localhost:3001".into()]); assert_eq!(m.discover("e").unwrap().len(),1); m.deregister("e",i.id).unwrap(); assert!(m.discover("e").is_err()); }
    #[test] fn test_mesh_heartbeat() { let m=ServiceMesh::new(); let i=m.register("e","1",vec![]); m.heartbeat("e",i.id,0.5).unwrap(); assert!(m.heartbeat("e",Uuid::new_v4(),0.5).is_err()); }
    #[test] fn test_engine() { let e=IntegrationEngine::new(); let c=Arc::new(TestComp{name:"t".into(),ok:true}); e.register(c).unwrap(); assert_eq!(e.component_count(),1); }
    #[test] fn test_engine_dup() { let e=IntegrationEngine::new(); let c=Arc::new(TestComp{name:"d".into(),ok:true}); e.register(c.clone()).unwrap(); assert!(e.register(c).is_err()); }
}
