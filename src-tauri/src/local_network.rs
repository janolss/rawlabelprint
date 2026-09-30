//! Trigger macOS Local Network privacy prompt via system Bonjour (DNS-SD).
//!
//! A Local Network toggle alone is not always enough for direct TCP to LAN
//! IPs until the process has browsed a Bonjour service listed in Info.plist.

#![cfg(target_os = "macos")]

use std::ffi::{c_void, CString};
use std::os::raw::{c_char, c_int};
use std::ptr;
use std::time::{Duration, Instant};

type DNSServiceRef = *mut c_void;
type DNSServiceErrorType = i32;
type DNSServiceFlags = u32;

type DNSServiceBrowseReply = Option<
    unsafe extern "C" fn(
        sd_ref: DNSServiceRef,
        flags: DNSServiceFlags,
        interface_index: u32,
        error_code: DNSServiceErrorType,
        service_name: *const c_char,
        regtype: *const c_char,
        reply_domain: *const c_char,
        context: *mut c_void,
    ),
>;

#[link(name = "System")]
extern "C" {
    fn DNSServiceBrowse(
        sd_ref: *mut DNSServiceRef,
        flags: DNSServiceFlags,
        interface_index: u32,
        regtype: *const c_char,
        domain: *const c_char,
        call_back: DNSServiceBrowseReply,
        context: *mut c_void,
    ) -> DNSServiceErrorType;

    fn DNSServiceProcessResult(sd_ref: DNSServiceRef) -> DNSServiceErrorType;
    fn DNSServiceRefDeallocate(sd_ref: DNSServiceRef);
    fn DNSServiceRefSockFD(sd_ref: DNSServiceRef) -> c_int;
}

unsafe extern "C" fn browse_reply(
    _sd_ref: DNSServiceRef,
    _flags: DNSServiceFlags,
    _interface_index: u32,
    error_code: DNSServiceErrorType,
    service_name: *const c_char,
    regtype: *const c_char,
    reply_domain: *const c_char,
    _context: *mut c_void,
) {
    if error_code != 0 {
        tracing::debug!("Bonjour browse reply error: {error_code}");
        return;
    }
    if service_name.is_null() || regtype.is_null() {
        return;
    }
    let name = unsafe { std::ffi::CStr::from_ptr(service_name) }.to_string_lossy();
    let ty = unsafe { std::ffi::CStr::from_ptr(regtype) }.to_string_lossy();
    let domain = if reply_domain.is_null() {
        String::new()
    } else {
        unsafe { std::ffi::CStr::from_ptr(reply_domain) }
            .to_string_lossy()
            .into_owned()
    };
    tracing::debug!("Bonjour found: {name} {ty} {domain}");
}

fn browse_service_type(regtype: &str, duration: Duration) {
    let Ok(c_regtype) = CString::new(regtype) else {
        return;
    };
    let mut sd_ref: DNSServiceRef = ptr::null_mut();
    let err = unsafe {
        DNSServiceBrowse(
            &mut sd_ref,
            0,
            0,
            c_regtype.as_ptr(),
            ptr::null(),
            Some(browse_reply),
            ptr::null_mut(),
        )
    };
    if err != 0 || sd_ref.is_null() {
        tracing::warn!("DNSServiceBrowse({regtype}) failed: {err}");
        return;
    }

    let fd = unsafe { DNSServiceRefSockFD(sd_ref) };
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        let mut pollfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut pollfd, 1, 200) };
        if ready > 0 {
            let perr = unsafe { DNSServiceProcessResult(sd_ref) };
            if perr != 0 {
                tracing::debug!("DNSServiceProcessResult: {perr}");
                break;
            }
        }
    }

    unsafe { DNSServiceRefDeallocate(sd_ref) };
}

/// Browse Bonjour types from Info.plist so macOS shows / activates Local Network access.
pub fn trigger_local_network_permission_prompt() {
    tracing::info!("Triggering macOS Local Network permission via Bonjour browse");
    // Must match NSBonjourServices in Info.plist
    for service in ["_printer._tcp", "_ipp._tcp", "_http._tcp"] {
        browse_service_type(service, Duration::from_millis(800));
    }
}
