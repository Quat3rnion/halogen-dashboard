use crate::{
    metrics::parse,
    rates::CounterTracker,
    storage::Store,
    types::{Event, Info, Snapshot, Status, now_ms},
};
use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Query, State},
    http::{StatusCode, Uri, header},
    response::{Html, IntoResponse, Response, Sse, sse::Event as SseEvent},
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    convert::Infallible,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{net::TcpListener, sync::broadcast, task::JoinHandle};

#[derive(rust_embed::RustEmbed)]
#[folder = "../../web/dist/"]
#[allow_missing = true]
struct Assets;

#[derive(Clone)]
pub struct App {
    inner: Arc<Mutex<Inner>>,
    tx: broadcast::Sender<(String, Value)>,
}
struct Inner {
    store: Store,
    tracker: CounterTracker,
    snapshot: Snapshot,
    gauges: std::collections::BTreeMap<String, f64>,
    health_status: Status,
    /// Why the Status is Degraded, for Event.detail. None when Up.
    health_detail: Option<String>,
    uptime_since: Option<i64>,
    last_scrape_ok: Option<i64>,
    outage: bool,
}

#[derive(Serialize, Deserialize)]
struct Checkpoint {
    tracker: CounterTracker,
    snapshot: Snapshot,
    uptime_since: Option<i64>,
    last_scrape_ok: Option<i64>,
    outage: bool,
}

impl App {
    pub fn new(db: impl AsRef<Path>) -> Result<Self> {
        let store = Store::open(db)?;
        store.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS checkpoint(id INTEGER PRIMARY KEY,data TEXT NOT NULL)",
        )?;
        let persisted = store
            .conn
            .query_row("SELECT data FROM checkpoint WHERE id=1", [], |r| {
                r.get::<_, String>(0)
            })
            .ok()
            .and_then(|s| serde_json::from_str::<Checkpoint>(&s).ok());
        let mut p = persisted.unwrap_or(Checkpoint {
            tracker: CounterTracker::default(),
            snapshot: Snapshot::default(),
            uptime_since: None,
            last_scrape_ok: None,
            outage: false,
        });
        p.snapshot.events = store.events(0, i64::MAX, true)?;
        let (tx, _) = broadcast::channel(256);
        Ok(Self {
            inner: Arc::new(Mutex::new(Inner {
                store,
                tracker: p.tracker,
                snapshot: p.snapshot,
                gauges: Default::default(),
                health_status: Status::Up,
                health_detail: None,
                uptime_since: p.uptime_since,
                last_scrape_ok: p.last_scrape_ok,
                outage: p.outage,
            })),
            tx,
        })
    }
    fn send<T: Serialize>(&self, kind: &str, value: &T) {
        if let Ok(value) = serde_json::to_value(value) {
            let _ = self.tx.send((kind.into(), value));
        }
    }
    fn record_event(&self, inner: &mut Inner, event: Event) -> Result<()> {
        inner.store.event(&event)?;
        self.send("event", &event);
        inner.snapshot.events.insert(0, event);
        inner.snapshot.events.truncate(100);
        Ok(())
    }
    pub fn scrape_metrics(&self, t: i64, text: Option<&str>) -> Result<()> {
        let mut i = self.inner.lock().unwrap();
        let old_status = i.snapshot.latest.as_ref().map(|s| s.status);
        let status = if text.is_some() {
            i.health_status
        } else {
            Status::Down
        };
        let (frame, restart) = i.tracker.sample(t, text, status);
        if restart {
            self.record_event(&mut i, Event::new(t, "restart"))?;
            i.uptime_since = Some(t);
        }
        if text.is_none() && !i.outage {
            let mut e = Event::new(t, "outage_start");
            e.detail = Some("Halogen /metrics did not answer".into());
            self.record_event(&mut i, e)?;
            i.outage = true;
        }
        if let Some(text) = text {
            if i.outage {
                self.record_event(&mut i, Event::new(t, "outage_end"))?;
                i.uptime_since = Some(t);
            }
            i.outage = false;
            i.last_scrape_ok = Some(t);
            if i.uptime_since.is_none() {
                i.uptime_since = Some(t);
            }
            i.gauges = parse(text);
        } else {
            i.gauges.clear();
        }
        if let Some(old) = old_status.filter(|old| *old != status) {
            let mut e = Event::new(t, "status_change");
            e.from = Some(old);
            e.to = Some(status);
            e.detail = match status {
                Status::Degraded => i.health_detail.clone(),
                Status::Down => Some("Halogen /metrics did not answer".into()),
                Status::Up => None,
            };
            self.record_event(&mut i, e)?;
        }
        let uptime = i.uptime_since;
        let last = i.last_scrape_ok;
        if let Some(info) = &mut i.snapshot.info {
            info.uptime_since = uptime;
            info.last_scrape_ok = last;
            self.send("info", info);
        }
        i.store.insert(&frame, true)?;
        i.store.flush()?;
        i.snapshot.latest = Some(frame.sample.clone());
        // Checkpoint the offsets as well as Samples, so Collector restarts preserve adjusted counters.
        let checkpoint = json!({"tracker":i.tracker,"snapshot":i.snapshot,"uptime_since":i.uptime_since,"last_scrape_ok":i.last_scrape_ok,"outage":i.outage});
        i.store.conn.execute(
            "INSERT OR REPLACE INTO checkpoint(id,data) VALUES(1,?1)",
            [checkpoint.to_string()],
        )?;
        self.send("sample", &frame.sample);
        Ok(())
    }
    fn scrape_health(&self, url: &str, health: Option<Value>, model: Option<String>) {
        let mut i = self.inner.lock().unwrap();
        let Some(h) = health else {
            i.health_status = Status::Degraded;
            i.health_detail = Some("Halogen /health did not answer".into());
            return;
        };
        i.health_detail = degraded_reason(&h);
        i.health_status = if i.health_detail.is_some() {
            Status::Degraded
        } else {
            Status::Up
        };
        let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
        let info = Info {
            halogen_url: url.into(),
            model: h["model"]
                .as_str()
                .map(str::to_owned)
                .or(model)
                .unwrap_or_default(),
            version_api: text(&h["version"]["api"]),
            version_engine: text(&h["version"]["engine"]),
            slots: h["slots"].as_u64().unwrap_or_default(),
            slot_ctx: h["slot_ctx"].as_u64().unwrap_or_default(),
            kv_pool: h["kv_pool_positions"].as_u64().unwrap_or_default(),
            drafter_default: text(&h["drafter_default"]),
            prompt_cache_cap_mb: h["prompt_cache"]["cap_mb"].as_f64().unwrap_or_default(),
            engine_responds: h["engine"]["responds"].as_bool().unwrap_or(false),
            engine_probe_s: h["engine"]["probe_s"].as_f64().unwrap_or_default(),
            npu: h.get("npu").filter(|n| !n.is_null()).map(|n| {
                let mut value = json!({});
                for key in ["status", "models", "loaded", "queued", "passes", "contexts"] {
                    value[key] = n[key].clone();
                }
                if let Some(error) = n.get("error") {
                    value["error"] = error.clone();
                }
                value
            }),
            uptime_since: i.uptime_since,
            last_scrape_ok: i.last_scrape_ok,
        };
        if i.snapshot.info.as_ref() != Some(&info) {
            self.send("info", &info);
            i.snapshot.info = Some(info);
        }
    }
    fn scrape_cache(&self, raw: Value) {
        let mut cache = json!({"t":now_ms(),"pool":{}});
        for name in [
            "entries",
            "max_entries",
            "bytes",
            "hits",
            "misses",
            "evicted",
            "refused",
            "hit_rate",
            "token_hit_rate",
            "prompt_tokens_saved",
        ] {
            cache[name] = raw.get(name).cloned().unwrap_or(json!(0));
        }
        for name in [
            "positions",
            "used",
            "busy_regions",
            "held_regions",
            "waiting_for_room",
            "relocated",
        ] {
            cache["pool"][name] = raw["pool"].get(name).cloned().unwrap_or(json!(0));
        }
        let mut i = self.inner.lock().unwrap();
        i.snapshot.cache = Some(cache.clone());
        self.send("cache", &cache);
    }
    pub fn router(&self) -> Router {
        Router::new()
            .route("/api/snapshot", get(snapshot))
            .route("/api/series", get(series))
            .route("/api/events", get(events))
            .route("/api/stream", get(stream))
            .route("/metrics", get(metrics))
            .route("/healthz", get(|| async { "ok" }))
            .fallback(spa)
            .with_state(self.clone())
    }
    pub fn start(&self, url: &str) -> Result<Vec<JoinHandle<()>>> {
        self.start_with_key(url, None)
    }
    /// Like `start`, but sends `Authorization: Bearer <api_key>` when Halogen requires a key.
    pub fn start_with_key(&self, url: &str, api_key: Option<&str>) -> Result<Vec<JoinHandle<()>>> {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(key) = api_key.map(str::trim).filter(|k| !k.is_empty()) {
            let mut value = reqwest::header::HeaderValue::from_str(&format!("Bearer {key}"))?;
            value.set_sensitive(true);
            headers.insert(reqwest::header::AUTHORIZATION, value);
        }
        let client = reqwest::Client::builder().default_headers(headers).build()?;
        let mut tasks = Vec::new();
        for (surface, period, timeout) in [("metrics", 1, 2), ("health", 2, 3), ("cache", 5, 3)] {
            let app = self.clone();
            let url = url.trim_end_matches('/').to_owned();
            let client = client.clone();
            tasks.push(tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(period));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let mut model = None;
                loop {
                    interval.tick().await;
                    let response = client
                        .get(format!("{url}/{surface}"))
                        .timeout(Duration::from_secs(timeout))
                        .send()
                        .await
                        .and_then(reqwest::Response::error_for_status);
                    match surface {
                        "metrics" => {
                            let text = match response {
                                Ok(r) => r.text().await.ok(),
                                Err(_) => None,
                            };
                            let text = text.filter(|t| {
                                let m = parse(t);
                                m.contains_key("llamacpp:prompt_tokens_total")
                                    && m.contains_key("llamacpp:tokens_predicted_total")
                            });
                            if let Err(e) = app.scrape_metrics(now_ms(), text.as_deref()) {
                                tracing::error!(error=%e,"Cannot store Sample");
                            }
                        }
                        "health" => {
                            let health = match response {
                                Ok(r) => r.json::<Value>().await.ok(),
                                Err(_) => None,
                            };
                            if model.is_none()
                                && let Ok(r) = client
                                    .get(format!("{url}/v1/models"))
                                    .timeout(Duration::from_secs(3))
                                    .send()
                                    .await
                                && let Ok(v) = r.json::<Value>().await
                            {
                                model = v["data"][0]["id"].as_str().map(str::to_owned);
                            }
                            app.scrape_health(&url, health, model.clone());
                        }
                        _ => {
                            if let Ok(r) = response
                                && let Ok(cache) = r.json::<Value>().await
                            {
                                app.scrape_cache(cache);
                            }
                        }
                    }
                }
            }));
        }
        let app = self.clone();
        tasks.push(tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(600));
            loop {
                interval.tick().await;
                if let Err(e) = app.inner.lock().unwrap().store.prune(now_ms()) {
                    tracing::error!(error=%e,"Cannot prune history");
                }
            }
        }));
        Ok(tasks)
    }
}

pub async fn serve(listener: TcpListener, app: App, url: &str, api_key: Option<&str>) -> Result<()> {
    let tasks = app.start_with_key(url, api_key)?;
    let result = axum::serve(listener, app.router())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    for task in tasks {
        task.abort();
    }
    result?;
    Ok(())
}

async fn snapshot(State(app): State<App>) -> Json<Snapshot> {
    Json(app.inner.lock().unwrap().snapshot.clone())
}
#[derive(Deserialize)]
struct Range {
    from: i64,
    to: i64,
    step: Option<i64>,
}
type ApiResult = std::result::Result<Json<Value>, (StatusCode, Json<Value>)>;
fn api_error(e: impl std::fmt::Display) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error":e.to_string()})),
    )
}
async fn series(State(app): State<App>, Query(q): Query<Range>) -> ApiResult {
    tokio::task::spawn_blocking(move || {
        app.inner
            .lock()
            .unwrap()
            .store
            .series(q.from, q.to, q.step, now_ms())
    })
    .await
    .map_err(api_error)?
    .map(Json)
    .map_err(api_error)
}
async fn events(State(app): State<App>, Query(q): Query<Range>) -> ApiResult {
    if q.to < q.from {
        return Err(api_error("from must not be after to"));
    }
    let events = app
        .inner
        .lock()
        .unwrap()
        .store
        .events(q.from, q.to, false)
        .map_err(api_error)?;
    Ok(Json(json!(events)))
}
async fn stream(
    State(app): State<App>,
) -> Sse<impl futures_core::Stream<Item = std::result::Result<SseEvent, Infallible>>> {
    let mut rx = app.tx.subscribe();
    let initial = app.inner.lock().unwrap().snapshot.clone();
    let stream = async_stream::stream! {
        let period=Duration::from_secs(15);
        let mut keepalive=tokio::time::interval_at(tokio::time::Instant::now()+period,period);
        yield Ok(SseEvent::default().event("snapshot").json_data(initial).unwrap());
        loop {tokio::select! {
            _=keepalive.tick()=>yield Ok(SseEvent::default().comment("keepalive")),
            message=rx.recv()=>match message {
            Ok((kind,value))=>yield Ok(SseEvent::default().event(kind).json_data(value).unwrap()),
            Err(broadcast::error::RecvError::Lagged(_))=>{let snapshot=app.inner.lock().unwrap().snapshot.clone();yield Ok(SseEvent::default().event("snapshot").json_data(snapshot).unwrap());},
            Err(broadcast::error::RecvError::Closed)=>break,
        }}}
    };
    Sse::new(stream)
}
async fn metrics(State(app): State<App>) -> impl IntoResponse {
    let i = app.inner.lock().unwrap();
    let mut text = String::new();
    for (n, v) in crate::rates::COUNTERS.iter().zip(i.tracker.adjusted) {
        text.push_str(&format!("# TYPE {n} counter\n{n} {v}\n"));
    }
    for (n, v) in &i.gauges {
        if !crate::rates::COUNTERS.contains(&n.as_str()) {
            text.push_str(&format!("# TYPE {n} gauge\n{n} {v}\n"));
        }
    }
    let current = i
        .snapshot
        .latest
        .as_ref()
        .map(|s| s.status)
        .unwrap_or(Status::Down);
    for (name, status) in [
        ("up", Status::Up),
        ("degraded", Status::Degraded),
        ("down", Status::Down),
    ] {
        text.push_str(&format!(
            "halogen_dashboard_status{{status=\"{name}\"}} {}\n",
            u8::from(status == current)
        ));
    }
    text.push_str(&format!(
        "halogen_dashboard_restarts_total {}\n",
        i.tracker.restarts
    ));
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        text,
    )
}
async fn spa(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let file = Assets::get(path)
        .map(|f| (path, f))
        .or_else(|| Assets::get("index.html").map(|f| ("index.html", f)));
    if let Some((path, file)) = file {
        (
            [(
                header::CONTENT_TYPE,
                mime_guess::from_path(path)
                    .first_or_octet_stream()
                    .to_string(),
            )],
            file.data.into_owned(),
        )
            .into_response()
    } else {
        Html("<!doctype html><title>Halogen Dashboard</title><p>Frontend not built. Build web/ and rebuild the Collector.</p>").into_response()
    }
}

/// The fault that makes a /health body Degraded, or None when all parts report ok.
pub(crate) fn degraded_reason(h: &Value) -> Option<String> {
    if h["engine"]["responds"] == false || (h["status"] != "ok" && !h["status"].is_null()) {
        let status = h["status"].as_str().unwrap_or("unknown");
        let why = h["engine"]["detail"]
            .as_str()
            .map(|d| format!(": {d}"))
            .unwrap_or_default();
        return Some(format!("engine {status}{why}"));
    }
    let npu = h.get("npu").filter(|n| !n.is_null())?;
    if npu["status"] == "ok" {
        return None;
    }
    let status = npu["status"].as_str().unwrap_or("unknown");
    let why = npu["error"]
        .as_str()
        .map(|e| format!(": {e}"))
        .unwrap_or_default();
    Some(format!("NPU {status}{why}"))
}

#[cfg(test)]
mod degraded_tests {
    use super::degraded_reason;
    use serde_json::json;

    #[test]
    fn fixture_health_is_up() {
        let h: serde_json::Value =
            serde_json::from_str(include_str!("../../../fixtures/halogen-0.17.3/health.json"))
                .unwrap();
        assert_eq!(degraded_reason(&h), None);
    }

    #[test]
    fn engine_and_npu_faults_give_reasons() {
        let h = json!({"status":"engine_unresponsive","engine":{"responds":false,"detail":"PING timeout"}});
        assert_eq!(
            degraded_reason(&h).unwrap(),
            "engine engine_unresponsive: PING timeout"
        );
        let h = json!({"status":"ok","engine":{"responds":true},
            "npu":{"status":"engine_stalled","error":"one job has held the NPU for 95 s"}});
        assert_eq!(
            degraded_reason(&h).unwrap(),
            "NPU engine_stalled: one job has held the NPU for 95 s"
        );
    }
}
