use crate::http_server::fetch::find_header_end;
use crate::http_server::resolve::BrowserDeviceRef;
use axum::http::{header, HeaderMap};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct WriteBody {
    device: BrowserDeviceRef,
    #[serde(default)]
    data: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug)]
pub(crate) struct ParsedWrite {
    pub device: BrowserDeviceRef,
    pub data: Vec<u8>,
    pub url: Option<String>,
}

/// BrowserPrint.js uses either JSON (`device.send` / `sendUrl`) or multipart
/// FormData with fields `json` + `blob` (`device.sendFile`).
pub(crate) fn parse_write_request(headers: &HeaderMap, body: &[u8]) -> Result<ParsedWrite, String> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if let Some(boundary) = multipart_boundary(content_type) {
        return parse_write_multipart(body, &boundary);
    }

    // Fallback: some clients omit Content-Type but still send JSON.
    if body.starts_with(b"--") {
        return Err("multipart body without boundary in Content-Type".into());
    }

    let parsed: WriteBody =
        serde_json::from_slice(body).map_err(|e| format!("Invalid JSON: {e}"))?;
    Ok(ParsedWrite {
        device: parsed.device,
        data: parsed.data.unwrap_or_default().into_bytes(),
        url: parsed.url,
    })
}

pub(crate) fn multipart_boundary(content_type: &str) -> Option<String> {
    let ct = content_type.trim();
    if !ct.to_ascii_lowercase().starts_with("multipart/") {
        return None;
    }
    for part in ct.split(';').skip(1) {
        let part = part.trim();
        let (k, v) = part.split_once('=')?;
        if k.eq_ignore_ascii_case("boundary") {
            let v = v.trim().trim_matches('"');
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn parse_write_multipart(body: &[u8], boundary: &str) -> Result<ParsedWrite, String> {
    let parts = parse_multipart_parts(body, boundary)?;
    let json_bytes = parts
        .get("json")
        .ok_or_else(|| "Missing multipart field 'json'".to_string())?;
    let meta: WriteBody =
        serde_json::from_slice(json_bytes).map_err(|e| format!("Invalid JSON: {e}"))?;

    let mut data = parts.get("blob").cloned().unwrap_or_default();
    if data.is_empty() {
        if let Some(d) = meta.data {
            data = d.into_bytes();
        }
    }

    Ok(ParsedWrite {
        device: meta.device,
        data,
        url: meta.url,
    })
}

/// Minimal multipart/form-data parser for BrowserPrint field names.
pub(crate) fn parse_multipart_parts(
    body: &[u8],
    boundary: &str,
) -> Result<HashMap<String, Vec<u8>>, String> {
    let delim = format!("--{boundary}").into_bytes();
    let mut map = HashMap::new();
    let mut pos = 0usize;

    while pos < body.len() {
        let Some(start) = find_bytes(&body[pos..], &delim).map(|i| pos + i) else {
            break;
        };
        let mut cursor = start + delim.len();
        if body.get(cursor..cursor + 2) == Some(b"--") {
            break; // closing boundary
        }
        if body.get(cursor..cursor + 2) == Some(b"\r\n") {
            cursor += 2;
        } else if body.get(cursor..cursor + 1) == Some(b"\n") {
            cursor += 1;
        }

        // find_header_end returns index of first body byte (after \r\n\r\n)
        let header_end = find_header_end(&body[cursor..])
            .map(|i| cursor + i)
            .ok_or_else(|| "Malformed multipart part (no header end)".to_string())?;
        let header_block_end = header_end.saturating_sub(4);
        let headers_text = std::str::from_utf8(&body[cursor..header_block_end])
            .map_err(|_| "Invalid multipart headers encoding".to_string())?;

        let name = multipart_field_name(headers_text)
            .ok_or_else(|| "Multipart part missing Content-Disposition name".to_string())?;

        let next_boundary = find_bytes(&body[header_end..], &delim)
            .map(|i| header_end + i)
            .unwrap_or(body.len());
        let mut value_end = next_boundary;
        if value_end >= 2 && &body[value_end - 2..value_end] == b"\r\n" {
            value_end -= 2;
        } else if value_end >= 1 && body[value_end - 1] == b'\n' {
            value_end -= 1;
        }

        map.insert(name, body[header_end..value_end].to_vec());
        pos = next_boundary;
    }

    if map.is_empty() {
        return Err("No multipart parts found".into());
    }
    Ok(map)
}

fn multipart_field_name(headers: &str) -> Option<String> {
    for line in headers.split('\n') {
        let line = line.trim().trim_end_matches('\r');
        let lower = line.to_ascii_lowercase();
        if !lower.starts_with("content-disposition:") {
            continue;
        }
        for part in line.split(';').skip(1) {
            let part = part.trim();
            let (k, v) = match part.split_once('=') {
                Some(kv) => kv,
                None => continue,
            };
            if k.eq_ignore_ascii_case("name") {
                return Some(v.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn multipart_boundary_extraction() {
        assert_eq!(
            multipart_boundary(
                "multipart/form-data; boundary=----WebKitFormBoundaryjCzCASXAXUOmmw2k"
            )
            .as_deref(),
            Some("----WebKitFormBoundaryjCzCASXAXUOmmw2k")
        );
        assert_eq!(
            multipart_boundary(r#"multipart/form-data; boundary="----QuotedBound""#).as_deref(),
            Some("----QuotedBound")
        );
        assert!(multipart_boundary("application/json").is_none());
    }

    #[test]
    fn parse_browserprint_sendfile_multipart() {
        let boundary = "----WebKitFormBoundaryjCzCASXAXUOmmw2k";
        let zpl = "^XA^FO50,50^FDHi^FS^XZ";
        let body = format!(
            "------WebKitFormBoundaryjCzCASXAXUOmmw2k\r\n\
Content-Disposition: form-data; name=\"json\"\r\n\r\n\
{{\"device\":{{\"name\":\"ZTC ZD421\",\"uid\":\"D6J231909982\",\"deviceType\":\"printer\"}}}}\r\n\
------WebKitFormBoundaryjCzCASXAXUOmmw2k\r\n\
Content-Disposition: form-data; name=\"blob\"; filename=\"blob\"\r\n\
Content-Type: text/plain\r\n\r\n\
{zpl}\r\n\
------WebKitFormBoundaryjCzCASXAXUOmmw2k--\r\n"
        );

        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}")).unwrap(),
        );

        let parsed = parse_write_request(&headers, body.as_bytes()).unwrap();
        assert_eq!(parsed.device.uid.as_deref(), Some("D6J231909982"));
        assert_eq!(String::from_utf8_lossy(&parsed.data), zpl);
    }

    #[test]
    fn parse_multipart_missing_json_errors() {
        let boundary = "bound";
        let body = format!(
            "--{boundary}\r\n\
Content-Disposition: form-data; name=\"blob\"\r\n\r\n\
^XA^XZ\r\n\
--{boundary}--\r\n"
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}")).unwrap(),
        );
        let err = parse_write_request(&headers, body.as_bytes()).unwrap_err();
        assert!(err.contains("json"));
    }

    #[test]
    fn parse_json_send_still_works() {
        let body = br#"{"device":{"uid":"ABC"},"data":"^XA^XZ"}"#;
        let headers = HeaderMap::new();
        let parsed = parse_write_request(&headers, body).unwrap();
        assert_eq!(parsed.device.uid.as_deref(), Some("ABC"));
        assert_eq!(String::from_utf8_lossy(&parsed.data), "^XA^XZ");
        assert!(parsed.url.is_none());
    }

    #[test]
    fn parse_json_send_url_field() {
        let body = br#"{"device":{"uid":"ABC"},"url":"http://example.com/label.zpl"}"#;
        let headers = HeaderMap::new();
        let parsed = parse_write_request(&headers, body).unwrap();
        assert_eq!(parsed.url.as_deref(), Some("http://example.com/label.zpl"));
        assert!(parsed.data.is_empty());
    }
}
