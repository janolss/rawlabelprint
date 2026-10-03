//! Minimal dependency-free HTTP GET for BrowserPrint sendUrl.

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Semaphore;

const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_CONCURRENT_FETCHES: usize = 4;
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

static FETCH_SLOTS: Semaphore = Semaphore::const_new(MAX_CONCURRENT_FETCHES);

pub(crate) async fn fetch_url_bytes(url: &str) -> Result<Vec<u8>, String> {
    fetch_url_bytes_with_limits(url, MAX_RESPONSE_BYTES, FETCH_TIMEOUT, &FETCH_SLOTS).await
}

async fn fetch_url_bytes_with_limits(
    url: &str,
    max_response_bytes: usize,
    timeout: Duration,
    slots: &Semaphore,
) -> Result<Vec<u8>, String> {
    tokio::time::timeout(timeout, async {
        let _permit = slots
            .acquire()
            .await
            .map_err(|e| format!("URL fetch unavailable: {e}"))?;
        fetch_url_bytes_inner(url, max_response_bytes).await
    })
    .await
    .map_err(|_| "URL fetch timed out".to_string())?
}

async fn fetch_url_bytes_inner(url: &str, max_response_bytes: usize) -> Result<Vec<u8>, String> {
    let url = url
        .strip_prefix("http://")
        .ok_or_else(|| "Only http:// URLs are supported for sendUrl".to_string())?;
    let (host_port, path) = match url.split_once('/') {
        Some((host, path)) => (host, format!("/{path}")),
        None => (url, "/".to_string()),
    };
    let (host, port) = match host_port.split_once(':') {
        Some((host, port)) => (host, port.parse::<u16>().unwrap_or(80)),
        None => (host_port, 80),
    };

    let mut stream = TcpStream::connect((host, port))
        .await
        .map_err(|e| e.to_string())?;
    let request = format!("GET {path} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| e.to_string())?;

    let max_wire_bytes = MAX_HEADER_BYTES.saturating_add(max_response_bytes);
    let mut response = Vec::with_capacity(max_wire_bytes.min(8192));
    let mut chunk = [0u8; 8192];
    let mut header_end = None;
    loop {
        let read = stream.read(&mut chunk).await.map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        response.extend_from_slice(&chunk[..read]);
        if response.len() > max_wire_bytes {
            return Err("URL response exceeded the maximum size".into());
        }

        if header_end.is_none() {
            header_end = find_header_end(&response);
            if header_end.is_none() && response.len() > MAX_HEADER_BYTES {
                return Err("URL response headers exceeded the maximum size".into());
            }
        }
        if let Some(end) = header_end {
            if end > MAX_HEADER_BYTES {
                return Err("URL response headers exceeded the maximum size".into());
            }
            if response.len() - end > max_response_bytes {
                return Err("URL response exceeded the maximum size".into());
            }
        }
    }

    if let Some(end) = header_end {
        response.drain(..end);
    }
    Ok(response)
}

pub(crate) fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    async fn response_server(
        response: &'static [u8],
        delay: Duration,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut chunk = [0u8; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = stream.read(&mut chunk).await.unwrap();
                if read == 0 {
                    return;
                }
                request.extend_from_slice(&chunk[..read]);
            }
            tokio::time::sleep(delay).await;
            let _ = stream.write_all(response).await;
        });
        (format!("http://{address}/label.zpl"), task)
    }

    #[tokio::test]
    async fn rejects_response_body_over_limit() {
        let (url, server) = response_server(
            b"HTTP/1.0 200 OK\r\nContent-Length: 9\r\n\r\n123456789",
            Duration::ZERO,
        )
        .await;
        let result =
            fetch_url_bytes_with_limits(&url, 8, Duration::from_secs(1), &Semaphore::const_new(1))
                .await;

        assert!(result.unwrap_err().contains("maximum size"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn times_out_slow_response() {
        let (url, server) = response_server(
            b"HTTP/1.0 200 OK\r\nContent-Length: 2\r\n\r\nok",
            Duration::from_millis(100),
        )
        .await;
        let result = fetch_url_bytes_with_limits(
            &url,
            8,
            Duration::from_millis(10),
            &Semaphore::const_new(1),
        )
        .await;

        assert_eq!(result.unwrap_err(), "URL fetch timed out");
        server.await.unwrap();
    }
}
