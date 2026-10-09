//! Minimal dependency-free HTTP GET for BrowserPrint sendUrl.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Semaphore;

struct HttpTarget {
    host: String,
    port: u16,
    path: String,
    addr: SocketAddr,
}

const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_CONCURRENT_FETCHES: usize = 4;
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

static FETCH_SLOTS: Semaphore = Semaphore::const_new(MAX_CONCURRENT_FETCHES);

pub(crate) async fn fetch_url_bytes(url: &str) -> Result<Vec<u8>, String> {
    let target = parse_and_check_url(url).await?;
    fetch_target_with_limits(&target, MAX_RESPONSE_BYTES, FETCH_TIMEOUT, &FETCH_SLOTS).await
}

fn reject_control_chars(value: &str) -> Result<(), String> {
    if value.chars().any(|c| c == '\r' || c == '\n' || c == '\0') {
        return Err("Invalid URL".into());
    }
    Ok(())
}

fn ip_allowed(ip: IpAddr) -> bool {
    let ip = match ip {
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(v6)),
        other => other,
    };
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_loopback()
                || v4.is_unspecified()
                || v4.is_link_local()
                || v4.is_multicast()
                || v4.is_broadcast())
        }
        IpAddr::V6(v6) => {
            let link_local = (v6.segments()[0] & 0xffc0) == 0xfe80;
            !(v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() || link_local)
        }
    }
}

fn parse_http_url(url: &str) -> Result<(String, u16, String), String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| "Only http:// URLs are supported for sendUrl".to_string())?;
    let (host_port, path) = match rest.split_once('/') {
        Some((host, path)) => (host, format!("/{path}")),
        None => (rest, "/".to_string()),
    };
    reject_control_chars(host_port)?;
    reject_control_chars(&path)?;
    let (host, port) =
        if let Some(host) = host_port.strip_prefix('[').and_then(|v| v.split_once(']')) {
            let port = host
                .1
                .strip_prefix(':')
                .map(|p| p.parse::<u16>().unwrap_or(80))
                .unwrap_or(80);
            (host.0.to_string(), port)
        } else {
            match host_port.rsplit_once(':') {
                Some((host, port)) if !host.contains(':') => {
                    (host.to_string(), port.parse::<u16>().unwrap_or(80))
                }
                _ => (host_port.to_string(), 80),
            }
        };
    if host.is_empty() {
        return Err("Invalid URL".into());
    }
    Ok((host, port, path))
}

async fn parse_and_check_url(url: &str) -> Result<HttpTarget, String> {
    let (host, port, path) = parse_http_url(url)?;
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|e| format!("Could not resolve URL host: {e}"))?
        .collect();
    if addrs.is_empty() {
        return Err("Could not resolve URL host".into());
    }
    if addrs.iter().any(|addr| !ip_allowed(addr.ip())) {
        return Err("URL host is not allowed".into());
    }
    Ok(HttpTarget {
        host,
        port,
        path,
        addr: addrs[0],
    })
}

async fn fetch_target_with_limits(
    target: &HttpTarget,
    max_response_bytes: usize,
    timeout: Duration,
    slots: &Semaphore,
) -> Result<Vec<u8>, String> {
    let target = HttpTarget {
        host: target.host.clone(),
        port: target.port,
        path: target.path.clone(),
        addr: target.addr,
    };
    tokio::time::timeout(timeout, async move {
        let _permit = slots
            .acquire()
            .await
            .map_err(|e| format!("URL fetch unavailable: {e}"))?;
        fetch_url_bytes_inner(&target, max_response_bytes).await
    })
    .await
    .map_err(|_| "URL fetch timed out".to_string())?
}

async fn fetch_url_bytes_inner(
    target: &HttpTarget,
    max_response_bytes: usize,
) -> Result<Vec<u8>, String> {
    let HttpTarget {
        host, path, addr, ..
    } = target;
    let mut stream = TcpStream::connect(*addr).await.map_err(|e| e.to_string())?;
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

    let Some(end) = header_end else {
        return Err("URL response has no headers".into());
    };
    let code = http_status_code(&response)?;
    if !(200..300).contains(&code) {
        return Err(format!("URL response status {code}"));
    }
    response.drain(..end);
    Ok(response)
}

fn http_status_code(buf: &[u8]) -> Result<u16, String> {
    let line_end = buf
        .iter()
        .position(|&b| b == b'\n')
        .ok_or_else(|| "URL response has no status line".to_string())?;
    let line = std::str::from_utf8(&buf[..line_end])
        .map_err(|_| "URL response status line is not UTF-8".to_string())?;
    let code = line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| "URL response status line is invalid".to_string())?;
    code.parse::<u16>()
        .map_err(|_| "URL response status line is invalid".to_string())
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
        let result = fetch_unchecked(&url, 8, Duration::from_secs(1)).await;

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
        let result = fetch_unchecked(&url, 8, Duration::from_millis(10)).await;

        assert_eq!(result.unwrap_err(), "URL fetch timed out");
        server.await.unwrap();
    }

    async fn fetch_unchecked(
        url: &str,
        max_bytes: usize,
        timeout: Duration,
    ) -> Result<Vec<u8>, String> {
        let (host, port, path) = parse_http_url(url)?;
        let addr = format!("{host}:{port}")
            .parse()
            .map_err(|e| format!("{e}"))?;
        let target = HttpTarget {
            host,
            port,
            path,
            addr,
        };
        fetch_target_with_limits(&target, max_bytes, timeout, &Semaphore::const_new(1)).await
    }

    #[test]
    fn rejects_loopback_and_control_characters() {
        assert!(!ip_allowed("127.0.0.1".parse().unwrap()));
        assert!(!ip_allowed("169.254.169.254".parse().unwrap()));
        assert!(!ip_allowed("0.0.0.0".parse().unwrap()));
        assert!(ip_allowed("10.1.2.3".parse().unwrap()));
        assert!(ip_allowed("192.168.1.50".parse().unwrap()));
        assert!(parse_http_url("http://printer.example/a\r\nX: y").is_err());
        assert!(parse_http_url("http://evil\n.example/x").is_err());
    }

    #[tokio::test]
    async fn rejects_non_success_status() {
        let (url, server) =
            response_server(b"HTTP/1.0 404 Not Found\r\n\r\nmissing", Duration::ZERO).await;
        let err = fetch_unchecked(&url, 64, Duration::from_secs(1))
            .await
            .unwrap_err();
        assert!(err.contains("404"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn public_fetch_rejects_loopback() {
        let err = fetch_url_bytes("http://127.0.0.1/label.zpl")
            .await
            .unwrap_err();
        assert!(err.contains("not allowed"));
    }
}
