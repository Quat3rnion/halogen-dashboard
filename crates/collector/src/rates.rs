use crate::{
    metrics::parse,
    types::{Sample, Status, Totals},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

pub const COUNTERS: [&str; 11] = [
    "halogen:requests_total",
    "llamacpp:prompt_tokens_total",
    "llamacpp:tokens_predicted_total",
    "halogen:prompt_tokens_cached_total",
    "halogen:draft_tokens_total",
    "halogen:draft_tokens_accepted_total",
    "halogen:structured_requests_total",
    "llamacpp:prompt_seconds_total",
    "llamacpp:tokens_predicted_seconds_total",
    "halogen:draft_list_tokens_total",
    "halogen:draft_list_misses_total",
];

pub const CACHE_COUNTERS: [&str; 6] = [
    "halogen:cache_hits_total",
    "halogen:cache_misses_total",
    "halogen:cache_evicted_total",
    "halogen:cache_last_message_rewrites_total",
    "halogen:cache_disk_hits_total",
    "halogen:cache_disk_cut_back_total",
];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Frame {
    pub sample: Sample,
    pub counters: [f64; 11],
    #[serde(default)]
    pub cache_counters: [Option<f64>; 6],
    pub kv_pool: Option<f64>,
    // Rates never bridge an Outage, even when the endpoint counters survive.
    pub segment: i64,
}

pub fn totals(c: &[f64; 11]) -> Totals {
    Totals {
        requests: c[0],
        prompt_tokens: c[1],
        predicted_tokens: c[2],
        cached_tokens: c[3],
        draft_tokens: c[4],
        draft_accepted: c[5],
        structured_requests: c[6],
        ..Default::default()
    }
}

pub fn cache_totals(totals: &mut Totals, c: &[f64; 6]) {
    totals.cache_hits = c[0];
    totals.cache_misses = c[1];
    totals.cache_evicted = c[2];
    totals.cache_rewrites = c[3];
    totals.cache_disk_hits = c[4];
    totals.cache_disk_cut_backs = c[5];
}

#[derive(Default, Serialize, Deserialize)]
pub struct CounterTracker {
    previous: BTreeMap<String, f64>,
    offsets: [f64; 11],
    pub adjusted: [f64; 11],
    #[serde(default)]
    cache_offsets: [f64; 6],
    #[serde(default)]
    pub cache_adjusted: [f64; 6],
    exact_seconds: [f64; 2],
    pub restarts: u64,
    history: VecDeque<Frame>,
    segment: i64,
    outage: bool,
}

impl CounterTracker {
    pub fn sample(&mut self, t: i64, text: Option<&str>, status: Status) -> (Frame, bool) {
        self.sample_values(t, text.map(parse).as_ref(), status)
    }

    pub fn sample_values(
        &mut self,
        t: i64,
        values: Option<&BTreeMap<String, f64>>,
        status: Status,
    ) -> (Frame, bool) {
        let Some(m) = values else {
            self.history.clear();
            self.outage = true;
            return (
                Frame {
                    sample: Sample {
                        t,
                        status: Status::Down,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                false,
            );
        };
        let restart = COUNTERS.iter().any(|name| {
            m.get(*name)
                .zip(self.previous.get(*name))
                .is_some_and(|(n, p)| n < p)
        });
        let initial = self.previous.is_empty();
        if initial || self.outage {
            self.segment = t;
        }
        if restart {
            self.restarts += 1;
            self.offsets = self.adjusted;
            self.cache_offsets = self.cache_adjusted;
            self.previous.clear();
        }
        let mut cache_counters = [None; 6];
        for (i, name) in CACHE_COUNTERS.iter().enumerate() {
            if let Some(value) = m.get(*name) {
                if self
                    .previous
                    .get(*name)
                    .is_some_and(|previous| value < previous)
                {
                    self.cache_offsets[i] = self.cache_adjusted[i];
                }
                self.cache_adjusted[i] = self.cache_offsets[i] + value;
                cache_counters[i] = Some(self.cache_adjusted[i]);
                self.previous.insert((*name).into(), *value);
            }
        }
        let before = self.adjusted;
        for (i, name) in COUNTERS.iter().enumerate() {
            if let Some(value) = m.get(*name) {
                self.adjusted[i] = self.offsets[i] + value;
            }
        }
        for (s, tokens, seconds, gauge) in [
            (0, 1, 7, "llamacpp:prompt_tokens_seconds"),
            (1, 2, 8, "llamacpp:predicted_tokens_seconds"),
        ] {
            let dt = self.adjusted[tokens] - before[tokens];
            let g = m.get(gauge).copied().unwrap_or(0.0);
            if initial {
                self.exact_seconds[s] = self.adjusted[seconds];
            } else {
                self.exact_seconds[s] += if g > 0.0 && !self.outage {
                    dt / g
                } else {
                    (self.adjusted[seconds] - before[seconds]).max(0.0)
                };
            }
        }
        for name in COUNTERS {
            if let Some(v) = m.get(name) {
                if let Some(previous) = self.previous.get_mut(name) {
                    *previous = *v;
                } else {
                    self.previous.insert(name.into(), *v);
                }
            }
        }
        self.outage = false;
        let mut counters = self.adjusted;
        counters[7] = self.exact_seconds[0];
        counters[8] = self.exact_seconds[1];
        let pool = m.get("halogen:kv_pool_positions").copied();
        let kv_used = m.get("llamacpp:kv_cache_tokens").copied();
        let mut totals = totals(&counters);
        cache_totals(&mut totals, &self.cache_adjusted);
        let mut frame = Frame {
            sample: Sample {
                t,
                status,
                busy_slots: m.get("llamacpp:requests_processing").copied(),
                queued: m.get("llamacpp:requests_deferred").copied(),
                kv_used,
                kv_reserved: m.get("halogen:kv_pool_reserved_tokens").copied(),
                kv_use: m.get("llamacpp:kv_cache_usage_ratio").copied(),
                totals: Some(totals),
                busy_for: m.get("halogen:busy_for_seconds").copied(),
                kv_waiting: m.get("halogen:kv_pool_waiting_for_room").copied(),
                cache_entries: m.get("halogen:cache_entries").copied(),
                cache_bytes: m.get("halogen:cache_bytes").copied(),
                cache_stats_age: m.get("halogen:cache_stats_age_seconds").copied(),
                cache_disk_enabled: m.get("halogen:cache_disk_enabled").copied(),
                cache_disk_bytes: m.get("halogen:cache_disk_bytes").copied(),
                cache_disk_budget: m.get("halogen:cache_disk_budget_bytes").copied(),
                ..Default::default()
            },
            counters,
            cache_counters,
            kv_pool: pool,
            segment: self.segment,
        };
        while self.history.len() > 1 && self.history[1].sample.t <= t - 30_000 {
            self.history.pop_front();
        }
        if let Some(first) = self.history.front() {
            let last = frame.clone();
            apply_rates(&mut frame.sample, first, &last, 30_000);
        }
        self.history.push_back(frame.clone());
        (frame, restart)
    }
}

pub fn ratio(n: f64, d: f64) -> Option<f64> {
    (d > 0.0).then_some(n / d)
}

pub fn apply_rates(sample: &mut Sample, first: &Frame, last: &Frame, _window_ms: i64) {
    if first.segment != last.segment
        || first.sample.status == Status::Down
        || last.sample.status == Status::Down
    {
        return;
    }
    let wall = (last.sample.t - first.sample.t) as f64 / 1000.0;
    if wall <= 0.0 {
        return;
    }
    let mut d = [0.0; 11];
    for (i, v) in d.iter_mut().enumerate() {
        *v = (last.counters[i] - first.counters[i]).max(0.0);
    }
    sample.gen_speed = ratio(d[2], d[8]);
    sample.throughput = Some(d[2] / wall);
    sample.prefill_speed = ratio(d[1], d[7]);
    sample.prefill_throughput = Some(d[1] / wall);
    sample.requests_per_min = Some(d[0] / wall * 60.0);
    sample.mean_service_time = ratio(d[7] + d[8], d[0]);
    sample.prompt_cache_hit_rate = ratio(d[3], d[3] + d[1]);
    sample.draft_acceptance = ratio(d[5], d[4]);
    let d: [Option<f64>; 6] = std::array::from_fn(|i| {
        first.cache_counters[i]
            .zip(last.cache_counters[i])
            .map(|(a, b)| (b - a).max(0.0))
    });
    sample.cache_lookup_hit_rate = d[0]
        .zip(d[1])
        .and_then(|(hits, misses)| ratio(hits, hits + misses));
    sample.cache_evictions_per_min = d[2].map(|v| v / wall * 60.0);
    sample.cache_rewrites_per_min = d[3].map(|v| v / wall * 60.0);
    sample.cache_disk_hits_per_min = d[4].map(|v| v / wall * 60.0);
    sample.cache_disk_cut_backs_per_min = d[5].map(|v| v / wall * 60.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_resets_are_adjusted_without_restart_events_and_survive_checkpoints() {
        let mut tracker = CounterTracker::default();
        let text = |requests, cache| {
            format!(
                "halogen:requests_total {requests}\n{}",
                CACHE_COUNTERS
                    .iter()
                    .map(|name| format!("{name} {cache}\n"))
                    .collect::<String>()
            )
        };
        tracker.sample(0, Some(&text(100, 100)), Status::Up);
        let (f, restart) = tracker.sample(10_000, Some(&text(101, 2)), Status::Up);
        assert!(!restart);
        assert_eq!(f.sample.totals.unwrap().cache_hits, 102.0);
        assert_eq!(f.sample.cache_evictions_per_min, Some(12.0));
        let mut tracker: CounterTracker =
            serde_json::from_str(&serde_json::to_string(&tracker).unwrap()).unwrap();
        let (f, restart) = tracker.sample(20_000, Some(&text(1, 1)), Status::Up);
        assert!(restart);
        assert_eq!(tracker.restarts, 1);
        assert_eq!(f.sample.totals.unwrap().cache_disk_cut_backs, 103.0);
        tracker.sample(30_000, Some("halogen:requests_total 2"), Status::Up);
        let (f, restart) = tracker.sample(40_000, Some(&text(3, 2)), Status::Up);
        assert!(!restart);
        assert_eq!(f.sample.totals.unwrap().cache_hits, 104.0);
        let sparse = text(4, 3).replace(
            "halogen:cache_evicted_total 3",
            "halogen:cache_evicted_total 0",
        );
        let (f, restart) = tracker.sample(50_000, Some(&sparse), Status::Up);
        assert!(!restart);
        assert_eq!(f.sample.totals.unwrap().cache_hits, 105.0);
        assert_eq!(f.sample.totals.unwrap().cache_evicted, 104.0);
        assert!(f.sample.cache_evictions_per_min.unwrap() >= 0.0);
    }
    #[test]
    fn cache_metrics_use_window_deltas_and_old_versions_remain_null() {
        let mut tracker = CounterTracker::default();
        tracker.sample(0, Some("halogen:cache_hits_total 10\nhalogen:cache_misses_total 10\nhalogen:cache_evicted_total 1"), Status::Up);
        let (frame, _) = tracker.sample(30_000, Some("halogen:cache_hits_total 16\nhalogen:cache_misses_total 12\nhalogen:cache_evicted_total 3\nhalogen:busy_for_seconds 9"), Status::Up);
        let value = serde_json::to_value(frame.sample).unwrap();
        assert_eq!(value["cache_lookup_hit_rate"], 0.75);
        assert_eq!(value["cache_evictions_per_min"], 4.0);
        assert_eq!(value["busy_for"], 9.0);
        let fixture = include_str!("../../../fixtures/halogen-0.17.3/metrics.txt");
        let mut old = CounterTracker::default();
        old.sample(0, Some(fixture), Status::Up);
        let (frame, _) = old.sample(30_000, Some(fixture), Status::Up);
        let value = serde_json::to_value(frame.sample).unwrap();
        assert!(
            value
                .as_object()
                .unwrap()
                .contains_key("cache_evictions_per_min")
        );
        assert!(value["cache_evictions_per_min"].is_null());
        for field in [
            "busy_for",
            "kv_waiting",
            "cache_entries",
            "cache_bytes",
            "cache_stats_age",
            "cache_disk_enabled",
            "cache_disk_bytes",
            "cache_disk_budget",
            "cache_lookup_hit_rate",
            "cache_evictions_per_min",
            "cache_rewrites_per_min",
            "cache_disk_hits_per_min",
            "cache_disk_cut_backs_per_min",
        ] {
            assert!(value[field].is_null(), "{field}");
        }
        let fixture = include_str!("../../../fixtures/halogen-0.18.1/metrics.txt");
        let (frame, _) = tracker.sample(60_000, Some(fixture), Status::Up);
        assert_eq!(frame.sample.cache_bytes, Some(1047506868.0));
        assert_eq!(frame.sample.cache_entries, Some(9.0));
        assert_eq!(frame.sample.cache_stats_age, Some(1.999));
        assert_eq!(frame.sample.cache_disk_enabled, Some(0.0));
        assert_eq!(frame.sample.cache_disk_bytes, Some(0.0));
        assert_eq!(frame.sample.cache_disk_budget, Some(0.0));
        assert_eq!(frame.sample.kv_waiting, Some(0.0));
    }
    fn text(c: &[f64; 11], ps: f64, gs: f64) -> String {
        let mut text = COUNTERS
            .iter()
            .zip(c)
            .map(|(n, v)| format!("{n} {v}\n"))
            .collect::<String>();
        text.push_str(&format!(
            "llamacpp:prompt_tokens_seconds {ps}\nllamacpp:predicted_tokens_seconds {gs}\n"
        ));
        text
    }
    #[test]
    fn kv_use_comes_from_halogen_usage_ratio() {
        let mut tracker = CounterTracker::default();
        let fixture = include_str!("../../../fixtures/halogen-0.17.3/metrics.txt");
        let (f, _) = tracker.sample(1000, Some(fixture), Status::Up);
        assert_eq!(f.sample.kv_use, Some(0.000244141));
        let (f, _) = tracker.sample(
            2000,
            Some("llamacpp:kv_cache_tokens 5\nhalogen:kv_pool_positions 10\n"),
            Status::Up,
        );
        assert_eq!(f.sample.kv_use, None);
    }
    #[test]
    fn counters_remain_adjusted_across_restart() {
        let mut tracker = CounterTracker::default();
        tracker.sample(1000, Some(&text(&[100.0; 11], 0.0, 0.0)), Status::Up);
        let (f, restart) = tracker.sample(2000, Some(&text(&[2.0; 11], 0.0, 0.0)), Status::Up);
        assert!(restart);
        assert_eq!(f.sample.totals.unwrap().requests, 102.0);
        let (f, restart) = tracker.sample(3000, Some(&text(&[3.0; 11], 0.0, 0.0)), Status::Up);
        assert!(!restart);
        assert_eq!(f.sample.totals.unwrap().requests, 103.0);
    }
    #[test]
    fn exact_windowed_gauges_override_rounded_seconds_and_all_rate_formulas() {
        let mut tracker = CounterTracker::default();
        let mut c = [0.0; 11];
        c[7] = 100_000.0;
        c[8] = 100_000.0;
        tracker.sample(1000, Some(&text(&c, 0.0, 0.0)), Status::Up);
        c[0] = 2.0;
        c[1] = 1000.0;
        c[2] = 100.0;
        c[3] = 1000.0;
        c[4] = 130.0;
        c[5] = 91.0;
        let (f, _) = tracker.sample(11_000, Some(&text(&c, 2000.0, 50.0)), Status::Up);
        let s = f.sample;
        assert_eq!(s.gen_speed, Some(50.0));
        assert_eq!(s.prefill_speed, Some(2000.0));
        assert_eq!(s.throughput, Some(10.0));
        assert_eq!(s.prefill_throughput, Some(100.0));
        assert_eq!(s.requests_per_min, Some(12.0));
        assert_eq!(s.mean_service_time, Some(1.25));
        assert_eq!(s.prompt_cache_hit_rate, Some(0.5));
        assert_eq!(s.draft_acceptance, Some(0.7));
    }
    #[test]
    fn idle_rates_and_outage_are_null_where_undefined() {
        let mut tracker = CounterTracker::default();
        let text = text(&[0.0; 11], 0.0, 0.0);
        tracker.sample(1000, Some(&text), Status::Up);
        let (f, _) = tracker.sample(2000, Some(&text), Status::Up);
        assert_eq!(f.sample.throughput, Some(0.0));
        assert_eq!(f.sample.requests_per_min, Some(0.0));
        assert_eq!(f.sample.gen_speed, None);
        assert_eq!(f.sample.prefill_speed, None);
        assert_eq!(f.sample.mean_service_time, None);
        assert_eq!(f.sample.prompt_cache_hit_rate, None);
        assert_eq!(f.sample.draft_acceptance, None);
        let (f, _) = tracker.sample(3000, None, Status::Down);
        assert!(f.sample.totals.is_none());
        assert!(f.sample.throughput.is_none());
        let (f, _) = tracker.sample(4000, Some(&text), Status::Up);
        assert_eq!(f.sample.throughput, None);
    }

    #[test]
    fn optional_counter_returning_after_restart_is_not_a_second_restart() {
        let mut tracker = CounterTracker::default();
        tracker.sample(1000, Some(&text(&[100.0; 11], 0.0, 0.0)), Status::Up);
        let sparse = text(&[2.0; 11], 0.0, 0.0)
            .lines()
            .filter(|l| !l.starts_with("halogen:draft_list_misses_total"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(tracker.sample(2000, Some(&sparse), Status::Up).1);
        let (frame, restart) = tracker.sample(3000, Some(&text(&[3.0; 11], 0.0, 0.0)), Status::Up);
        assert!(!restart);
        assert_eq!(frame.counters[10], 103.0);
    }
}
