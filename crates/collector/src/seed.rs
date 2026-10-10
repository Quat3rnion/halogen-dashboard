use crate::{
    rates::CounterTracker,
    simulator::Simulator,
    storage::{RAW_MS, Store},
    types::{Event, Status, now_ms},
};
use anyhow::{Result, ensure};
use std::path::Path;

pub fn seed(path: impl AsRef<Path>, days: u32, seed: u64) -> Result<()> {
    ensure!((1..=30).contains(&days), "days must be between 1 and 30");
    let now = now_ms() / 1000 * 1000;
    let start = now - i64::from(days) * 86_400_000;
    let mut store = Store::open(path)?;
    let mut model = Simulator::new(seed, 1.0);
    let mut tracker = CounterTracker::default();
    let mut previous = Status::Up;
    let mut outage = false;
    let mut latest = None;
    let mut uptime_since = Some(start);
    store.conn.execute_batch("BEGIN IMMEDIATE")?;
    for t in (start..=now).step_by(1000) {
        let elapsed = (t - start) / 1000;
        if elapsed > 0 && elapsed % 259_200 == 0 {
            model.control("restart");
        }
        if elapsed > 0 && elapsed % 43_200 == 3600 {
            model.control("outage");
        }
        if elapsed > 0 && elapsed % 21_600 == 7200 {
            model.control("degrade");
        }
        model.step(1.0);
        let down = model.is_outage();
        let status = if down {
            Status::Down
        } else if model.is_degraded() {
            Status::Degraded
        } else {
            Status::Up
        };
        let values = (!down).then(|| model.metric_values());
        let (frame, restart) = tracker.sample_values(t, values.as_ref(), status);
        if restart {
            store.event(&Event::new(t, "restart"))?;
            uptime_since = Some(t);
        }
        if down && !outage {
            let mut e = Event::new(t, "outage_start");
            e.detail = Some("Halogen /metrics did not answer".into());
            store.event(&e)?;
        }
        if !down && outage {
            store.event(&Event::new(t, "outage_end"))?;
            uptime_since = Some(t);
        }
        if status != previous {
            let mut e = Event::new(t, "status_change");
            e.from = Some(previous);
            e.to = Some(status);
            e.detail = match status {
                Status::Degraded => crate::collector::degraded_reason(&model.health()),
                Status::Down => Some("Halogen /metrics did not answer".into()),
                Status::Up => None,
            };
            store.event(&e)?;
        }
        store.insert(&frame, t >= now - RAW_MS)?;
        latest = Some(frame.sample);
        previous = status;
        outage = down;
    }
    store.flush()?;
    store.prune(now)?;
    let snapshot = crate::types::Snapshot {
        latest,
        events: store.events(0, now, true)?,
        ..Default::default()
    };
    let checkpoint = serde_json::json!({"tracker":tracker,"snapshot":snapshot,"uptime_since":uptime_since,"last_scrape_ok":now,"outage":outage});
    store.conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS checkpoint(id INTEGER PRIMARY KEY,data TEXT NOT NULL)",
    )?;
    store.conn.execute(
        "INSERT OR REPLACE INTO checkpoint(id,data) VALUES(1,?1)",
        [checkpoint.to_string()],
    )?;
    store
        .conn
        .execute_batch("COMMIT; PRAGMA wal_checkpoint(TRUNCATE)")?;
    Ok(())
}
