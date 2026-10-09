use crate::config::PrinterInfo;
use crate::http_server::HttpSharedState;
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
pub(crate) struct BrowserDeviceRef {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub uid: Option<String>,
}

pub(crate) async fn list_printers_json(state: &HttpSharedState) -> String {
    let printers = collect_known_printers(state).await;
    let list: Vec<_> = printers
        .iter()
        .map(|p| json!({ "Name": p.display_name() }))
        .collect();
    serde_json::to_string(&list).unwrap_or_else(|_| "[]".into())
}

pub(crate) async fn collect_known_printers(state: &HttpSharedState) -> Vec<PrinterInfo> {
    let config = state.config.read().await;
    let mut by_key: BTreeMap<String, PrinterInfo> = BTreeMap::new();
    for p in config.added_printers.iter() {
        by_key.insert(p.address.clone(), p.clone());
    }
    if let Some(p) = &config.default_printer {
        by_key.insert(p.address.clone(), p.clone());
    }
    by_key.into_values().collect()
}

fn unique_name_match(printers: &[PrinterInfo], query: &str) -> Option<PrinterInfo> {
    let hits: Vec<PrinterInfo> = printers
        .iter()
        .filter(|p| {
            p.display_name().eq_ignore_ascii_case(query)
                || p.name
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(query))
        })
        .cloned()
        .collect();
    if hits.len() == 1 {
        hits.into_iter().next()
    } else {
        None
    }
}

pub(crate) async fn resolve_printer(
    state: &HttpSharedState,
    printer_name: &str,
) -> Option<PrinterInfo> {
    let config = state.config.read().await;
    let query = printer_name.trim();
    if query.is_empty() {
        return config.default_printer.clone();
    }

    let printers = {
        let mut by_key: BTreeMap<String, PrinterInfo> = BTreeMap::new();
        for p in config.added_printers.iter() {
            by_key.insert(p.address.clone(), p.clone());
        }
        if let Some(p) = &config.default_printer {
            by_key.insert(p.address.clone(), p.clone());
        }
        by_key.into_values().collect::<Vec<_>>()
    };

    if let Some(p) = printers.iter().find(|p| {
        p.address.eq_ignore_ascii_case(query) || p.browser_print_uid().eq_ignore_ascii_case(query)
    }) {
        return Some(p.clone());
    }
    unique_name_match(&printers, query)
}

pub(crate) async fn resolve_by_device_ref(
    state: &HttpSharedState,
    device: &BrowserDeviceRef,
) -> Option<PrinterInfo> {
    let uid = device.uid.as_deref().unwrap_or("").trim();
    let name = device.name.as_deref().unwrap_or("").trim();

    let printers = collect_known_printers(state).await;
    if !uid.is_empty() {
        if let Some(p) = printers.iter().find(|p| p.browser_print_uid() == uid) {
            return Some(p.clone());
        }
        // Also accept raw address or net:host:port forms
        if let Some(rest) = uid.strip_prefix("net:") {
            let host = rest.split(':').next().unwrap_or(rest);
            if let Some(p) = printers.iter().find(|p| p.address == host) {
                return Some(p.clone());
            }
        }
        if let Some(p) = printers
            .iter()
            .find(|p| p.address == uid || p.serial_number == uid)
        {
            return Some(p.clone());
        }
    }
    if !name.is_empty() {
        return unique_name_match(&printers, name);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_server::test_util::{sample_printer, test_state};

    #[tokio::test]
    async fn resolve_printer_by_empty_name_uses_default() {
        let printer = sample_printer("10.0.0.1", "SN1");
        let state = test_state(true, Some(printer.clone()));
        let resolved = resolve_printer(&state, "").await;
        assert_eq!(resolved.unwrap().address, "10.0.0.1");
    }

    #[tokio::test]
    async fn resolve_by_device_ref_uid_serial_and_net() {
        let printer = sample_printer("10.0.0.1", "SN1");
        let state = test_state(true, Some(printer.clone()));

        let by_serial = resolve_by_device_ref(
            &state,
            &BrowserDeviceRef {
                name: None,
                uid: Some("SN1".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(by_serial.address, "10.0.0.1");

        let by_net = resolve_by_device_ref(
            &state,
            &BrowserDeviceRef {
                name: None,
                uid: Some("net:10.0.0.1:9100".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(by_net.serial_number, "SN1");

        let by_name = resolve_by_device_ref(
            &state,
            &BrowserDeviceRef {
                name: Some("Printer 10.0.0.1".into()),
                uid: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(by_name.address, "10.0.0.1");

        let miss = resolve_by_device_ref(
            &state,
            &BrowserDeviceRef {
                name: Some("nope".into()),
                uid: Some("missing".into()),
            },
        )
        .await;
        assert!(miss.is_none());
    }

    #[tokio::test]
    async fn resolve_printer_does_not_match_model_alone() {
        let state = test_state(true, None);
        {
            let mut cfg = state.config.write().await;
            cfg.upsert_printer(sample_printer("10.0.0.1", "SN1"), true);
            cfg.upsert_printer(sample_printer("10.0.0.2", "SN2"), false);
        }
        assert!(resolve_printer(&state, "ZD421").await.is_none());
        let by_address = resolve_printer(&state, "10.0.0.2").await.unwrap();
        assert_eq!(by_address.serial_number, "SN2");
        let listed = collect_known_printers(&state).await;
        assert_eq!(listed[0].address, "10.0.0.1");
        assert_eq!(listed[1].address, "10.0.0.2");
    }
}
