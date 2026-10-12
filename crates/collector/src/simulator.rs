//! Deterministic, completion-accounted Halogen 0.18.1 simulator.
use std::{collections::VecDeque, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use rand::{Rng, SeedableRng, rngs::StdRng};
use rand_distr::{Distribution, LogNormal, Poisson};
use serde_json::{Value, json};
use tokio::sync::Mutex;

const METRICS: &str = include_str!("../../../fixtures/halogen-0.18.1/metrics.txt");
const POOL: u64 = 1_048_576;
// Stable internal accounting order, independent of the fixture's line order.
const NAMES: [&str; 19] = [
    "llamacpp:prompt_tokens_total",
    "llamacpp:prompt_seconds_total",
    "llamacpp:tokens_predicted_total",
    "llamacpp:tokens_predicted_seconds_total",
    "llamacpp:prompt_tokens_seconds",
    "llamacpp:predicted_tokens_seconds",
    "llamacpp:requests_processing",
    "llamacpp:requests_deferred",
    "llamacpp:kv_cache_tokens",
    "llamacpp:kv_cache_usage_ratio",
    "halogen:requests_total",
    "halogen:prompt_tokens_cached_total",
    "halogen:draft_tokens_total",
    "halogen:draft_tokens_accepted_total",
    "halogen:structured_requests_total",
    "halogen:draft_list_tokens_total",
    "halogen:draft_list_misses_total",
    "halogen:kv_pool_positions",
    "halogen:kv_pool_reserved_tokens",
];

struct Request {
    started: f64,
    prompt: u64,
    cached: u64,
    output: u64,
    generated: f64,
    prefill_left: f64,
    prefill_seconds: f64,
    generation_seconds: f64,
    generation_rate: f64,
    acceptance: f64,
    structured: bool,
    shortlist: bool,
}

pub struct Simulator {
    rng: StdRng,
    speed: f64,
    time: f64,
    outage_until: f64,
    degrade_until: f64,
    engine_fault: bool,
    burst_until: f64,
    active: Vec<Request>,
    queue: VecDeque<Request>,
    // NAMES order: the first four and positions 10..17 are counters.
    totals: [f64; 19],
    window: [f64; 4],
    hits: u64,
    misses: u64,
    passes: u64,
    health_template: Value,
    cache_template: Value,
    models_template: Value,
    prompt_distribution: LogNormal<f64>,
    output_distribution: LogNormal<f64>,
}

impl Simulator {
    pub fn new(seed: u64, speed: f64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            speed: if speed.is_finite() && speed > 0.0 {
                speed
            } else {
                1.0
            },
            time: 0.0,
            outage_until: 0.0,
            degrade_until: 0.0,
            engine_fault: false,
            burst_until: 0.0,
            active: Vec::with_capacity(4),
            queue: VecDeque::new(),
            totals: [0.0; 19],
            window: [0.0; 4],
            hits: 0,
            misses: 0,
            passes: 0,
            health_template: serde_json::from_str(include_str!(
                "../../../fixtures/halogen-0.18.1/health.json"
            ))
            .unwrap(),
            cache_template: serde_json::from_str(include_str!(
                "../../../fixtures/halogen-0.18.1/cache.json"
            ))
            .unwrap(),
            models_template: serde_json::from_str(include_str!(
                "../../../fixtures/halogen-0.18.1/v1_models.json"
            ))
            .unwrap(),
            prompt_distribution: LogNormal::new(8.8, 1.25).unwrap(),
            output_distribution: LogNormal::new(6.3, 1.0).unwrap(),
        }
    }

    fn request(&mut self) -> Request {
        let prompt = self
            .prompt_distribution
            .sample(&mut self.rng)
            .clamp(200.0, 120_000.0) as u64;
        let cached = if self.totals[10] > 0.0 && self.rng.random_bool(0.7) {
            (prompt as f64 * self.rng.random_range(0.05..=0.9)) as u64
        } else {
            0
        };
        let prefill_seconds = (prompt - cached) as f64 / self.rng.random_range(1500.0..3500.0);
        Request {
            started: self.time,
            prompt,
            cached,
            output: self
                .output_distribution
                .sample(&mut self.rng)
                .clamp(50.0, 8000.0) as u64,
            generated: 0.0,
            prefill_left: prefill_seconds,
            prefill_seconds: 0.0,
            generation_seconds: 0.0,
            generation_rate: self.rng.random_range(35.0..65.0),
            acceptance: self.rng.random_range(0.6..0.85),
            structured: self.rng.random_bool(0.05),
            shortlist: self.rng.random_bool(0.015),
        }
    }

    pub fn step(&mut self, dt: f64) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let mut left = dt;
        // Bound the integration step so time-stepped history retains Slot dynamics.
        while left > 0.0 {
            let dt = left.min(1.0);
            left -= dt;
            self.time += dt;
            if self.is_outage() {
                continue;
            }
            let phase = self.time % 1800.0;
            let day = (self.time / 86400.0 * std::f64::consts::TAU).sin();
            let rate = if self.time < self.burst_until || phase > 1740.0 {
                0.8
            } else if phase < 180.0 {
                0.0
            } else {
                0.015 + 0.05 * (day + 1.0) / 2.0
            };
            if rate > 0.0 {
                let arrivals = Poisson::new(rate * dt * self.speed)
                    .unwrap()
                    .sample(&mut self.rng) as usize;
                for _ in 0..arrivals {
                    let request = self.request();
                    self.queue.push_back(request);
                }
            }
            while self.active.len() < 4 {
                let Some(mut request) = self.queue.pop_front() else {
                    break;
                };
                request.started = self.time;
                self.active.push(request);
            }
            let concurrency = self.active.len() as f64;
            let slowdown = if self.time < self.degrade_until {
                0.15
            } else {
                1.0
            };
            let mut index = 0;
            while index < self.active.len() {
                let r = &mut self.active[index];
                let available = dt * slowdown;
                let prefill = available.min(r.prefill_left);
                r.prefill_left -= prefill;
                r.prefill_seconds += prefill / slowdown;
                let rate = r.generation_rate / (1.0 + 0.12 * (concurrency - 1.0));
                let generation = (available - prefill).min((r.output as f64 - r.generated) / rate);
                r.generated += generation * rate;
                r.generation_seconds += generation / slowdown;
                if r.generated + 1e-8 >= r.output as f64 {
                    let r = self.active.swap_remove(index);
                    self.complete(r);
                } else {
                    index += 1;
                }
            }
        }
    }

    fn complete(&mut self, r: Request) {
        let values = [
            (r.prompt - r.cached) as f64,
            r.prefill_seconds,
            r.output as f64,
            r.generation_seconds,
        ];
        for (i, value) in values.into_iter().enumerate() {
            self.totals[i] += value;
            self.window[i] += value;
        }
        self.totals[10] += 1.0;
        self.totals[11] += r.cached as f64;
        let draft = (r.output as f64 * 1.3) as u64;
        self.totals[12] += draft as f64;
        self.totals[13] += (draft as f64 * r.acceptance) as u64 as f64;
        self.totals[14] += u64::from(r.structured) as f64;
        if r.shortlist {
            self.totals[15] += draft as f64;
            self.totals[16] += (draft / 30) as f64;
        }
        if r.cached > 0 {
            self.hits += 1;
        } else {
            self.misses += 1;
        }
        self.passes += 1 + r.output / 64;
    }

    fn entries(&self) -> u64 {
        (self.totals[10] as u64).min(24)
    }
    fn used(&self) -> u64 {
        (self.entries() * 8192
            + self
                .active
                .iter()
                .map(|r| r.prompt + r.generated as u64)
                .sum::<u64>())
        .min(POOL)
    }
    fn reserved(&self) -> u64 {
        self.active
            .iter()
            .chain(self.queue.iter())
            .map(|r| r.prompt + r.output)
            .sum()
    }

    fn values(&mut self) -> [f64; 19] {
        let mut values = self.totals;
        values[4] = ratio(self.window[0], self.window[1]);
        values[5] = ratio(self.window[2], self.window[3]);
        values[6] = self.active.len() as f64;
        values[7] = self.queue.len() as f64;
        values[8] = self.used() as f64;
        values[9] = self.used() as f64 / POOL as f64;
        values[17] = POOL as f64;
        values[18] = self.reserved() as f64;
        self.window = [0.0; 4];
        values
    }

    /// Direct model values for history seeding; consumes the same gauge window as HTTP.
    pub fn metric_values(&mut self) -> std::collections::BTreeMap<String, f64> {
        let mut values: std::collections::BTreeMap<String, f64> = NAMES
            .into_iter()
            .map(str::to_owned)
            .zip(self.values())
            .collect();
        for (name, value) in [
            ("halogen:busy_for_seconds", self.busy_for()),
            (
                "halogen:kv_pool_waiting_for_room",
                if self.reserved() > POOL {
                    self.queue.len() as f64
                } else {
                    0.0
                },
            ),
            ("halogen:cache_entries", self.entries() as f64),
            ("halogen:cache_bytes", (self.entries() * 4_000_000) as f64),
            ("halogen:cache_stats_age_seconds", self.time.fract()),
            ("halogen:cache_hits_total", self.hits as f64),
            ("halogen:cache_misses_total", self.misses as f64),
            (
                "halogen:cache_evicted_total",
                (self.totals[10] - 24.0).max(0.0),
            ),
            (
                "halogen:cache_last_message_rewrites_total",
                (self.hits / 5) as f64,
            ),
            ("halogen:cache_prompt_tokens_saved_total", self.totals[11]),
            ("halogen:cache_disk_enabled", 0.0),
            ("halogen:cache_disk_bytes", 0.0),
            ("halogen:cache_disk_budget_bytes", 0.0),
            ("halogen:cache_disk_hits_total", 0.0),
            ("halogen:cache_disk_cut_back_total", 0.0),
            (
                "halogen:engine_up",
                f64::from(!(self.is_degraded() && self.engine_fault)),
            ),
        ] {
            values.insert(name.into(), value);
        }
        values
    }

    pub fn metrics(&mut self) -> String {
        let values = self.metric_values();
        let mut out = String::with_capacity(METRICS.len() + 200);
        for line in METRICS.lines() {
            if line.starts_with('#') {
                out.push_str(line);
            } else if let Some((name, _)) = line.split_once(' ') {
                out.push_str(name);
                out.push(' ');
                if name.starts_with("halogen:build_info{") {
                    out.push('1');
                } else {
                    let value = values[name];
                    if matches!(
                        name,
                        "llamacpp:prompt_seconds_total"
                            | "llamacpp:tokens_predicted_seconds_total"
                            | "llamacpp:prompt_tokens_seconds"
                            | "llamacpp:predicted_tokens_seconds"
                            | "llamacpp:kv_cache_usage_ratio"
                    ) {
                        out.push_str(&six_significant(value));
                    } else {
                        out.push_str(&value.to_string());
                    }
                }
            }
            out.push('\n');
        }
        out
    }

    fn busy_for(&self) -> f64 {
        self.active
            .iter()
            .map(|request| self.time - request.started)
            .fold(0.0, f64::max)
    }

    pub fn health(&self) -> Value {
        let mut h = self.health_template.clone();
        let degraded = self.time < self.degrade_until;
        h["status"] = json!(if degraded && self.engine_fault {
            "engine_unresponsive"
        } else {
            "ok"
        });
        h["engine"]["responds"] = json!(!(degraded && self.engine_fault));
        h["engine"]["probe_s"] = json!(if degraded && self.engine_fault {
            3.0
        } else {
            0.002
        });
        h["busy"] = json!(!self.active.is_empty());
        h["in_flight"] = json!(self.active.len());
        h["busy_for_s"] = json!(self.busy_for());
        h["queued"] = json!(self.queue.len());
        h["npu"]["passes"] = json!(self.passes);
        h["npu"]["queued"] = json!(self.queue.len() / 4);
        let loaded = if self.passes == 0 {
            vec![]
        } else {
            let names = h["npu"]["models"].as_array().unwrap();
            vec![names[(self.time as usize / 90) % names.len()].clone()]
        };
        h["npu"]["loaded"] = json!(loaded);
        if degraded && !self.engine_fault {
            h["npu"]["status"] = json!("engine_stalled");
            h["npu"]["running_s"] = json!(60.0);
            h["npu"]["error"] = json!("one job has held the NPU for 60 s (simulated)");
        }
        h
    }

    pub fn cache(&self) -> Value {
        let mut c = self.cache_template.clone();
        let stores = self.totals[10] as u64;
        for (key, value) in [
            ("entries", self.entries()),
            ("bytes", self.entries() * 4_000_000),
            ("last_entry_bytes", if stores > 0 { 4_000_000 } else { 0 }),
            ("hits", self.hits),
            ("misses", self.misses),
            ("stores", stores),
            ("evicted", stores.saturating_sub(24)),
            ("prompt_tokens_saved", self.totals[11] as u64),
        ] {
            c[key] = json!(value);
        }
        c["store_ms_total"] = json!(stores as f64 * 8.0);
        c["restore_ms_total"] = json!(self.hits as f64 * 2.0);
        c["last_store_ms"] = json!(if stores > 0 { 8.0 } else { 0.0 });
        c["last_restore_ms"] = json!(if self.hits > 0 { 2.0 } else { 0.0 });
        c["hit_rate"] = json!(ratio(self.hits as f64, (self.hits + self.misses) as f64));
        c["token_hit_rate"] = json!(ratio(self.totals[11], self.totals[0] + self.totals[11]));
        c["pool"]["used"] = json!(self.used());
        c["pool"]["busy_regions"] = json!(self.active.len());
        c["pool"]["held_regions"] = json!(self.entries());
        c["pool"]["waiting_for_room"] = json!(if self.reserved() > POOL {
            self.queue.len()
        } else {
            0
        });
        c["pool"]["relocated"] = json!(stores / 17);
        c["pool"]["usage_ratio"] = json!(self.used() as f64 / POOL as f64);
        c
    }

    pub fn models(&self) -> Value {
        self.models_template.clone()
    }

    pub fn control(&mut self, cmd: &str) -> bool {
        match cmd {
            "outage" => self.outage_until = self.time + self.rng.random_range(20.0..60.0),
            "degrade" => {
                self.degrade_until = self.time + self.rng.random_range(30.0..90.0);
                self.engine_fault = self.rng.random_bool(0.5);
            }
            "burst" => self.burst_until = self.time + 60.0,
            "restart" => {
                self.totals = [0.0; 19];
                self.window = [0.0; 4];
                self.hits = 0;
                self.misses = 0;
                self.passes = 0;
                self.active.clear();
                self.queue.clear();
                self.outage_until = 0.0;
                self.degrade_until = 0.0;
                self.burst_until = 0.0;
            }
            _ => return false,
        }
        true
    }

    pub fn is_outage(&self) -> bool {
        self.time < self.outage_until
    }

    pub fn is_degraded(&self) -> bool {
        self.time < self.degrade_until
    }
}

fn ratio(n: f64, d: f64) -> f64 {
    if d > 0.0 { n / d } else { 0.0 }
}

// Python's %.6g: six significant digits, with exponential notation at its bounds.
fn six_significant(value: f64) -> String {
    if value == 0.0 {
        return "0".into();
    }
    let scientific = format!("{value:.5e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap();
    let exponent: i32 = exponent.parse().unwrap();
    if !(-4..6).contains(&exponent) {
        format!(
            "{}e{:+03}",
            mantissa.trim_end_matches('0').trim_end_matches('.'),
            exponent
        )
    } else {
        let result = format!("{:.*}", (5 - exponent) as usize, value);
        if result.contains('.') {
            result
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string()
        } else {
            result
        }
    }
}

type Shared = Arc<Mutex<Simulator>>;

async fn surface(State(state): State<Shared>, uri: axum::http::Uri) -> Response {
    let mut sim = state.lock().await;
    if sim.is_outage() {
        // A failing body terminates the HTTP connection, rather than returning a valid Scrape.
        let stream = async_stream::stream! {
            yield Err::<String, _>(std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "simulated outage"));
        };
        return ([(header::CONNECTION, "close")], Body::from_stream(stream)).into_response();
    }
    match uri.path() {
        "/metrics" => (
            [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
            sim.metrics(),
        )
            .into_response(),
        "/health" => Json(sim.health()).into_response(),
        "/cache" => Json(sim.cache()).into_response(),
        _ => Json(sim.models()).into_response(),
    }
}

async fn control(State(state): State<Shared>, Path(cmd): Path<String>) -> StatusCode {
    if state.lock().await.control(&cmd) {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

pub async fn serve(
    listener: tokio::net::TcpListener,
    seed: u64,
    speed: f64,
    chaos: bool,
) -> anyhow::Result<()> {
    let state = Arc::new(Mutex::new(Simulator::new(seed, speed)));
    let ticking = state.clone();
    let ticker = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        let mut last = tokio::time::Instant::now();
        let mut rng = StdRng::seed_from_u64(seed ^ 0x4348_414f_5300);
        let mut next_chaos = rng.random_range(180.0..360.0);
        loop {
            interval.tick().await;
            let now = tokio::time::Instant::now();
            let mut sim = ticking.lock().await;
            sim.step((now - last).as_secs_f64());
            last = now;
            if chaos && sim.time >= next_chaos {
                sim.control(["outage", "restart", "degrade"][rng.random_range(0..3)]);
                next_chaos = sim.time + rng.random_range(180.0..360.0);
            }
        }
    });
    let app = Router::new()
        .route("/metrics", get(surface))
        .route("/health", get(surface))
        .route("/cache", get(surface))
        .route("/v1/models", get(surface))
        .route("/_sim/{cmd}", post(control))
        .with_state(state);
    let result = axum::serve(listener, app).await;
    ticker.abort();
    result?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_metrics_and_oldest_request_age_follow_load() {
        let mut sim = Simulator::new(42, 1.0);
        sim.control("burst");
        sim.step(60.0);
        let output = sim.metrics();
        assert!(output.contains("halogen:build_info{version=\"0.18.1\""));
        let metrics = crate::metrics::parse(&output);
        assert!(metrics["halogen:busy_for_seconds"] > 0.0);
        assert_eq!(metrics["halogen:cache_hits_total"], sim.hits as f64);
        assert_eq!(metrics["halogen:cache_misses_total"], sim.misses as f64);
        sim.step(3600.0);
        let metrics = sim.metric_values();
        assert!(metrics["halogen:cache_hits_total"] > 0.0);
        assert!(metrics["halogen:cache_misses_total"] > 0.0);
        assert!(metrics["halogen:cache_evicted_total"] > 0.0);
        assert!(metrics["halogen:cache_last_message_rewrites_total"] > 0.0);
        assert_eq!(metrics["halogen:cache_disk_enabled"], 0.0);
        assert_eq!(metrics["halogen:cache_disk_hits_total"], 0.0);
    }
    fn values(text: &str) -> Vec<(&str, f64)> {
        text.lines()
            .filter(|l| !l.starts_with('#'))
            .map(|l| {
                let (name, value) = l.split_once(' ').unwrap();
                (name, value.parse().unwrap())
            })
            .collect()
    }

    #[test]
    fn fixture_names_and_headers() {
        let output = Simulator::new(1, 1.0).metrics();
        assert_eq!(
            values(&output).iter().map(|v| v.0).collect::<Vec<_>>(),
            values(METRICS).iter().map(|v| v.0).collect::<Vec<_>>()
        );
        assert_eq!(
            output
                .lines()
                .filter(|l| l.starts_with('#'))
                .collect::<Vec<_>>(),
            METRICS
                .lines()
                .filter(|l| l.starts_with('#'))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn completion_accounting_and_window_reset() {
        let mut sim = Simulator::new(42, 1.0);
        let r = sim.request();
        sim.active.push(r);
        sim.step(0.001);
        assert_eq!(sim.totals[10], 0.0);
        sim.step(3600.0);
        let output = sim.metrics();
        let first = crate::metrics::parse(&output);
        assert!(first[NAMES[4]] > 0.0 && first[NAMES[5]] > 0.0);
        let reset = sim.metrics();
        let reset = crate::metrics::parse(&reset);
        assert_eq!((reset[NAMES[4]], reset[NAMES[5]]), (0.0, 0.0));
        assert_eq!(reset[NAMES[0]], first[NAMES[0]]);
    }

    #[test]
    fn monotonic_and_restart() {
        let mut sim = Simulator::new(1, 1.0);
        let indices = [0, 1, 2, 3, 10, 11, 12, 13, 14, 15, 16];
        for _ in 0..7200 {
            let previous = sim.totals;
            sim.step(1.0);
            for i in indices {
                assert!(sim.totals[i] >= previous[i]);
            }
        }
        assert!(sim.totals[10] > 0.0);
        assert!(sim.control("restart"));
        for i in indices {
            assert_eq!(sim.totals[i], 0.0);
        }
        assert_eq!(sim.cache()["stores"], 0);
        assert_eq!(sim.health()["npu"]["loaded"], json!([]));
    }

    #[test]
    fn determinism_and_controls() {
        let mut a = Simulator::new(17, 1.0);
        let mut b = Simulator::new(17, 1.0);
        for cmd in ["burst", "degrade", "outage", "restart"] {
            assert!(a.control(cmd));
            assert!(b.control(cmd));
            for _ in 0..600 {
                a.step(1.0);
                b.step(1.0);
            }
            assert_eq!(a.metrics(), b.metrics());
            assert_eq!(a.health(), b.health());
            assert_eq!(a.cache(), b.cache());
        }
        assert!(!a.control("unknown"));
        a.control("burst");
        a.step(60.0);
        assert_eq!(a.active.len(), 4);
        assert!(!a.queue.is_empty());
        a.control("outage");
        assert!(a.is_outage());
        a.step(61.0);
        assert!(!a.is_outage());
        a.control("degrade");
        let h = a.health();
        assert!(h["status"] == "engine_unresponsive" || h["npu"]["status"] == "engine_stalled");
        a.step(91.0);
        assert_eq!(a.health()["status"], "ok");
    }

    #[test]
    fn python_float_format() {
        for (value, expected) in [
            (0.0, "0"),
            (0.000244140625, "0.000244141"),
            (1234567.0, "1.23457e+06"),
            (999999.9, "1e+06"),
            (12.345678, "12.3457"),
        ] {
            assert_eq!(six_significant(value), expected);
        }
    }

    #[test]
    fn direct_values_share_the_http_window_and_speed_changes_load_not_time() {
        let mut sim = Simulator::new(42, 10.0);
        sim.control("outage");
        sim.step(19.0);
        assert!(sim.is_outage());
        sim.control("restart");
        sim.control("burst");
        sim.step(120.0);
        let direct = sim.metric_values();
        assert!(direct["llamacpp:predicted_tokens_seconds"] > 0.0);
        let text = sim.metrics();
        let http = crate::metrics::parse(&text);
        assert_eq!(http[NAMES[5]], 0.0);
        assert_eq!(http[NAMES[10]], direct["halogen:requests_total"]);
    }

    #[test]
    #[cfg_attr(debug_assertions, ignore = "release performance check")]
    fn forty_eight_hours() {
        let start = std::time::Instant::now();
        let mut sim = Simulator::new(1, 1.0);
        for _ in 0..172800 {
            sim.step(1.0);
        }
        assert!(start.elapsed().as_secs_f64() < 20.0);
    }
}
