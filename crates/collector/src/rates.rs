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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Frame {
    pub sample: Sample,
    pub counters: [f64; 11],
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
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct CounterTracker {
    previous: BTreeMap<String, f64>,
    offsets: [f64; 11],
    pub adjusted: [f64; 11],
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
            self.previous.clear();
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
        let mut frame = Frame {
            sample: Sample {
                t,
                status,
                busy_slots: m.get("llamacpp:requests_processing").copied(),
                queued: m.get("llamacpp:requests_deferred").copied(),
                kv_used,
                kv_reserved: m.get("halogen:kv_pool_reserved_tokens").copied(),
                kv_use: kv_used.zip(pool).and_then(|(a, b)| ratio(a, b)),
                totals: Some(totals(&counters)),
                ..Default::default()
            },
            counters,
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
}

#[cfg(test)]
mod tests {
    use super::*;
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
