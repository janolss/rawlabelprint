//! Minimal dependency-free HTTP GET for BrowserPrint sendUrl.

pub(crate) async fn fetch_url_bytes(url: &str) -> Result<Vec<u8>, String> {
    // Prefer reqwest if added later. Only http:// with a simple blocking fetch.
    let url = url.to_string();
    tokio::task::spawn_blocking(move || {
        use std::io::Read;
        use std::net::TcpStream;
        use std::time::Duration;

        let url = url
            .strip_prefix("http://")
            .ok_or_else(|| "Only http:// URLs are supported for sendUrl".to_string())?;
        let (host_port, path) = match url.split_once('/') {
            Some((h, p)) => (h, format!("/{p}")),
            None => (url, "/".to_string()),
        };
        let (host, port) = match host_port.split_once(':') {
            Some((h, p)) => (h, p.parse::<u16>().unwrap_or(80)),
            None => (host_port, 80),
        };
        let addr = format!("{host}:{port}");
        let mut stream = TcpStream::connect(addr).map_err(|e| e.to_string())?;
        stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(10))).ok();
        let req = format!("GET {path} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n");
        use std::io::Write;
        stream
            .write_all(req.as_bytes())
            .map_err(|e| e.to_string())?;
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        if let Some(pos) = find_header_end(&buf) {
            Ok(buf[pos..].to_vec())
        } else {
            Ok(buf)
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

pub(crate) fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}
