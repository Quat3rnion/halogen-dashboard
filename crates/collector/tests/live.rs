use halogen_dashboard::{collector::App, simulator, types::now_ms};
use serde_json::Value;
use std::time::Duration;

#[tokio::test]
async fn old_frames_rollups_and_checkpoint_serve_http_series_after_upgrade() {
    use halogen_dashboard::{
        rates::{CounterTracker, Frame},
        storage::Rollup,
    };
    use serde_json::json;
    let db = format!(
        "/arboretum/tmp/old-shape-{}-{}.sqlite",
        std::process::id(),
        now_ms()
    );
    drop(App::new(&db).unwrap());
    let sample = json!({"t": 0, "status": "up", "busy_slots": 2, "totals": {
        "requests":110, "prompt_tokens":0, "predicted_tokens":0, "cached_tokens":0,
        "draft_tokens":0, "draft_accepted":0, "structured_requests":0
    }});
    let frame =
        json!({"sample":sample, "counters":[110,0,0,0,0,0,0,0,0,0,0], "kv_pool":100, "segment":0});
    let rollup = json!({"t":0, "stats":vec![json!({"min":2,"max":2,"sum":2,"count":1});15], "status":"up", "first":frame, "last":frame});
    let tracker = json!({"previous":{"halogen:requests_total":10}, "offsets":[100,0,0,0,0,0,0,0,0,0,0], "adjusted":[110,0,0,0,0,0,0,0,0,0,0], "exact_seconds":[0,0], "restarts":2, "history":[frame], "segment":0, "outage":false});
    let _: Frame = serde_json::from_value(frame.clone()).unwrap();
    let _: Rollup = serde_json::from_value(rollup.clone()).unwrap();
    let _: CounterTracker = serde_json::from_value(tracker.clone()).unwrap();
    let checkpoint = json!({"tracker":tracker,"snapshot":{"info":null,"cache":null,"latest":sample,"events":[]},"uptime_since":0,"last_scrape_ok":0,"outage":false});
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute("INSERT INTO raw(t,data) VALUES(0,?1)", [frame.to_string()])
        .unwrap();
    let recent = now_ms();
    let mut recent_frame = frame.clone();
    recent_frame["sample"]["t"] = json!(recent);
    conn.execute(
        "INSERT INTO raw(t,data) VALUES(?1,?2)",
        rusqlite::params![recent, recent_frame.to_string()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO rollups(t,data) VALUES(0,?1)",
        [rollup.to_string()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO checkpoint(id,data) VALUES(1,?1)",
        [checkpoint.to_string()],
    )
    .unwrap();
    drop(conn);
    let app = App::new(&db).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let router = app.router();
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let response = reqwest::get(format!("{url}/api/series?from=0&to=60000&step=60000"))
        .await
        .unwrap();
    assert!(response.status().is_success());
    let series: Value = response.json().await.unwrap();
    assert_eq!(series["busy_slots"][0], 2.0);
    assert!(series["cache_evictions_per_min"][0].is_null());
    assert!(series["busy_for_max"][0].is_null());
    let response = reqwest::get(format!(
        "{url}/api/series?from={recent}&to={recent}&step=1000"
    ))
    .await
    .unwrap();
    assert!(response.status().is_success());
    let series: Value = response.json().await.unwrap();
    assert_eq!(series["source"], "raw");
    assert_eq!(series["busy_slots"][0], 2.0);
    assert!(series["cache_lookup_hit_rate"][0].is_null());
    app.scrape_metrics(now_ms(), Some("halogen:requests_total 11"))
        .unwrap();
    let snapshot: Value = reqwest::get(format!("{url}/api/snapshot"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(snapshot["latest"]["totals"]["requests"], 111.0);
    assert_eq!(snapshot["latest"]["totals"]["cache_hits"], 0.0);
    task.abort();
    let _ = task.await;
    drop(app);
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{db}{suffix}"));
    }
}

#[tokio::test]
async fn completed_requests_produce_generation_speed_through_http_series() {
    let fake = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", fake.local_addr().unwrap());
    let fake_task = tokio::spawn(simulator::serve(fake, 42, 1.0, false));
    let client = reqwest::Client::new();
    client
        .post(format!("{url}/_sim/burst"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let db = format!(
        "/arboretum/halogen-dashboard/test-{}-{}.sqlite",
        std::process::id(),
        now_ms()
    );
    let app = App::new(&db).unwrap();
    let scrapes = app.start(&url).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let collector = format!("http://{}", listener.local_addr().unwrap());
    let router = app.router();
    let collector_task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let start = now_ms();
    let result = tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let v: Value = client
                .get(format!(
                    "{collector}/api/series?from={start}&to={}",
                    now_ms()
                ))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            if v["gen_speed"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v.as_f64().is_some_and(|v| v > 0.0))
            {
                break;
            }
        }
    })
    .await;
    for task in scrapes {
        task.abort();
    }
    collector_task.abort();
    fake_task.abort();
    drop(app);
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{db}{suffix}"));
    }
    assert!(
        result.is_ok(),
        "no completed request produced generation speed in 90 seconds"
    );
}

#[tokio::test]
async fn independent_health_and_cache_failures_do_not_create_an_outage() {
    use axum::{Router, http::StatusCode, routing::get};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let halogen = Router::new()
        .route(
            "/metrics",
            get(|| async { include_str!("../../../fixtures/halogen-0.17.3/metrics.txt") }),
        )
        .route("/health", get(|| async { StatusCode::SERVICE_UNAVAILABLE }))
        .route("/cache", get(|| async { StatusCode::SERVICE_UNAVAILABLE }))
        .route(
            "/v1/models",
            get(|| async { axum::Json(serde_json::json!({"data":[]})) }),
        );
    let halogen_task = tokio::spawn(async move {
        axum::serve(listener, halogen).await.unwrap();
    });
    let app = App::new(":memory:").unwrap();
    let scrapes = app.start(&url).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let router = app.router();
    let collector_task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(2200)).await;
    let snapshot: Value = reqwest::get(format!("{url}/api/snapshot"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    for task in scrapes {
        task.abort();
    }
    halogen_task.abort();
    collector_task.abort();
    assert_eq!(snapshot["latest"]["status"], "degraded");
    assert!(snapshot["latest"]["totals"].is_object());
    assert!(snapshot["cache"].is_null());
    assert!(
        !snapshot["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "outage_start")
    );
}

#[tokio::test]
async fn sse_sends_keepalive_even_while_samples_arrive() {
    let app = App::new(":memory:").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/api/stream", listener.local_addr().unwrap());
    let router = app.router();
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let samples = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            app.scrape_metrics(
                now_ms(),
                Some(include_str!("../../../fixtures/halogen-0.17.3/metrics.txt")),
            )
            .unwrap();
        }
    });
    let mut response = reqwest::get(url).await.unwrap();
    let mut received = String::new();
    let result = tokio::time::timeout(Duration::from_secs(18), async {
        while let Some(chunk) = response.chunk().await.unwrap() {
            received.push_str(&String::from_utf8_lossy(&chunk));
            if received.contains(": keepalive") {
                break;
            }
        }
    })
    .await;
    samples.abort();
    task.abort();
    assert!(result.is_ok());
    assert!(received.contains(": keepalive"));
    assert!(received.contains("event: sample"));
    assert!(received.starts_with("event: snapshot"));
}

#[tokio::test]
async fn api_key_is_sent_as_bearer_token() {
    use axum::{Router, http::HeaderMap, http::StatusCode, response::IntoResponse, routing::get};
    async fn metrics(headers: HeaderMap) -> axum::response::Response {
        if headers.get("authorization").and_then(|v| v.to_str().ok()) == Some("Bearer secret") {
            include_str!("../../../fixtures/halogen-0.17.3/metrics.txt").into_response()
        } else {
            StatusCode::UNAUTHORIZED.into_response()
        }
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let halogen = Router::new().route("/metrics", get(metrics));
    let halogen_task = tokio::spawn(async move {
        axum::serve(listener, halogen).await.unwrap();
    });
    let mut latest = Vec::new();
    for key in [None, Some(" secret
")] {
        let app = App::new(":memory:").unwrap();
        let scrapes = app.start_with_key(&url, key).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let collector = format!("http://{}", listener.local_addr().unwrap());
        let router = app.router();
        let collector_task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let snapshot: Value = reqwest::get(format!("{collector}/api/snapshot"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        for task in scrapes {
            task.abort();
        }
        collector_task.abort();
        latest.push(snapshot["latest"].clone());
    }
    halogen_task.abort();
    assert!(!latest[0]["totals"].is_object(), "metrics read without the key");
    assert!(latest[1]["totals"].is_object(), "key not sent as a Bearer token");
}
