// Minimal /metrics server using Tokio TCP; requires features: tokio + metrics
// Run with: cargo run --example metrics_server --features "tokio metrics"

#[cfg(all(feature = "tokio", feature = "metrics"))]
#[tokio::main]
async fn main() -> std::io::Result<()> {
    use proc_daemon::metrics::MetricsCollector;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // Create a metrics collector and populate some sample metrics
    let metrics = MetricsCollector::new();
    metrics.set_gauge("proc_example_gauge", 42);
    metrics.increment_counter("proc_example_counter", 7);
    {
        let _t = proc_daemon::metrics::Timer::new(metrics.clone(), "proc_example_timer_seconds");
        // simulate some work
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let listener = TcpListener::bind("127.0.0.1:9898").await?;
    println!("metrics server listening on http://127.0.0.1:9898/metrics");

    loop {
        let (mut socket, _addr) = listener.accept().await?;
        let metrics = metrics.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await; // ignore errors for minimalism

            // Very naive HTTP parsing, check for GET /metrics
            let req = String::from_utf8_lossy(&buf);
            let path_ok = req.starts_with("GET /metrics ")
                || req.starts_with("GET /metrics\r\n")
                || req.contains("GET /metrics HTTP/");

            if path_ok {
                let body = metrics.get_metrics().render_prometheus();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(resp.as_bytes()).await;
            } else {
                let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
                let _ = socket.write_all(resp.as_bytes()).await;
            }
            let _ = socket.shutdown().await;
        });
    }
}

#[cfg(not(all(feature = "tokio", feature = "metrics")))]
fn main() {
    eprintln!("This example requires features: tokio + metrics");
}
