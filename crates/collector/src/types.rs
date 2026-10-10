use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Up,
    Degraded,
    Down,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Totals {
    pub requests: f64,
    pub prompt_tokens: f64,
    pub predicted_tokens: f64,
    pub cached_tokens: f64,
    pub draft_tokens: f64,
    pub draft_accepted: f64,
    pub structured_requests: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Sample {
    pub t: i64,
    pub status: Status,
    pub busy_slots: Option<f64>,
    pub queued: Option<f64>,
    pub kv_used: Option<f64>,
    pub kv_reserved: Option<f64>,
    pub kv_use: Option<f64>,
    pub gen_speed: Option<f64>,
    pub throughput: Option<f64>,
    pub prefill_speed: Option<f64>,
    pub prefill_throughput: Option<f64>,
    pub requests_per_min: Option<f64>,
    pub mean_service_time: Option<f64>,
    pub prompt_cache_hit_rate: Option<f64>,
    pub draft_acceptance: Option<f64>,
    pub totals: Option<Totals>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub t: i64,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<Status>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<Status>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Event {
    pub fn new(t: i64, kind: &str) -> Self {
        Self {
            t,
            kind: kind.into(),
            from: None,
            to: None,
            detail: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Info {
    pub halogen_url: String,
    pub model: String,
    pub version_api: String,
    pub version_engine: String,
    pub slots: u64,
    pub slot_ctx: u64,
    pub kv_pool: u64,
    pub drafter_default: String,
    pub prompt_cache_cap_mb: f64,
    pub engine_responds: bool,
    pub engine_probe_s: f64,
    pub npu: Option<Value>,
    pub uptime_since: Option<i64>,
    pub last_scrape_ok: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub info: Option<Info>,
    pub cache: Option<Value>,
    pub latest: Option<Sample>,
    pub events: Vec<Event>,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
