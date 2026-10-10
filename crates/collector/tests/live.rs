use halogen_dashboard::{collector::App, simulator, types::now_ms};
use serde_json::Value;
use std::time::Duration;

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
