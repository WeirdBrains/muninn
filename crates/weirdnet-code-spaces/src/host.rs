//! The host side of the plugin ABI: the table a plugin's run gets, over an application's
//! [`Host`]. Requests and replies are text (JSON by convention); what the methods are and
//! what they read and write is the application's.
use crate::abi::{CALL_ERROR, CALL_OK, WnBuf, WnHost, WnStr};
use std::ffi::c_void;

/// An application's host methods for one run.
pub trait Host {
    /// Handle one call: the method's name and the request text; the reply text, or the
    /// error message the plugin gets.
    fn call(&mut self, method: &str, request: &str) -> Result<String, String>;
}

/// Run `run` (a loaded plugin's run function) with `host` behind a host table that says
/// `abi_version`. Returns the plugin's code ([`crate::abi::RUN_OK`], `RUN_ERROR`,
/// `RUN_PANIC`, or anything else it returned).
///
/// # Safety
/// `run` must be a run function the application loaded and approved (see
/// [`crate::approved`]), of the ABI the table's version says; it runs as native code with
/// the process's rights.
pub unsafe fn run_with(abi_version: u32, run: crate::abi::PluginRun, host: &mut dyn Host) -> i32 {
    let mut h: &mut dyn Host = host;
    let table = WnHost { abi_version, ctx: &mut h as *mut &mut dyn Host as *mut c_void, call: host_call, free: host_free };
    // SAFETY: the caller vouches for `run`; `table` and `h` outlive the call.
    unsafe { run(&table) }
}

fn reply(out: *mut WnBuf, s: String) {
    let b = s.into_bytes().into_boxed_slice();
    let len = b.len();
    let ptr = Box::into_raw(b) as *mut u8;
    // SAFETY: `out` is the plugin's buffer for this call.
    unsafe { *out = WnBuf { ptr, len, cap: len } };
}

/// `WnHost::call`.
unsafe extern "C" fn host_call(ctx: *mut c_void, method: WnStr, request: WnStr, out: *mut WnBuf) -> i32 {
    if ctx.is_null() || out.is_null() {
        return CALL_ERROR;
    }
    // SAFETY: the plugin passes back the ctx we gave it, and lends the strings for this call.
    let (host, method, request) = unsafe { (&mut *(ctx as *mut &mut dyn Host), method.as_bytes(), request.as_bytes()) };
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<String, String> {
        let method = std::str::from_utf8(method).map_err(|_| "invalid host method: not UTF-8".to_string())?;
        let text = std::str::from_utf8(request).map_err(|_| "invalid request: not UTF-8".to_string())?;
        host.call(method, text)
    }));
    match res {
        Ok(Ok(s)) => {
            reply(out, s);
            CALL_OK
        }
        Ok(Err(e)) => {
            reply(out, e);
            CALL_ERROR
        }
        Err(_) => {
            reply(out, "the host failed on this call".into());
            CALL_ERROR
        }
    }
}

/// `WnHost::free`.
unsafe extern "C" fn host_free(_ctx: *mut c_void, buf: WnBuf) {
    if !buf.ptr.is_null() {
        // SAFETY: made by `reply` from a boxed slice of exactly `len` bytes.
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(buf.ptr, buf.len)) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::{RUN_ERROR, RUN_OK};

    struct Echo(Vec<String>);
    impl Host for Echo {
        fn call(&mut self, method: &str, request: &str) -> Result<String, String> {
            self.0.push(format!("{method}:{request}"));
            match method {
                "boom" => panic!("no"),
                "fail" => Err("nope".into()),
                _ => Ok(format!("[{request}]")),
            }
        }
    }

    fn call(h: &WnHost, m: &[u8], r: &str) -> (i32, String) {
        let mut out = WnBuf::EMPTY;
        let m = WnStr { ptr: m.as_ptr(), len: m.len() };
        // SAFETY: a valid table and strings for the call
        unsafe {
            let c = (h.call)(h.ctx, m, WnStr::new(r), &mut out);
            let t = String::from_utf8(out.as_slice().to_vec()).unwrap();
            (h.free)(h.ctx, out);
            (c, t)
        }
    }

    impl WnBuf {
        fn as_slice(&self) -> &[u8] {
            // SAFETY: test buffers come from `reply`
            unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
        }
    }

    unsafe extern "C" fn plugin(h: *const WnHost) -> i32 {
        // SAFETY: the table run_with made
        let h = unsafe { &*h };
        assert_eq!(h.abi_version, 7);
        assert_eq!(call(h, b"echo", "{}"), (CALL_OK, "[{}]".into()));
        assert_eq!(call(h, b"fail", "x"), (CALL_ERROR, "nope".into()));
        assert_eq!(call(h, b"boom", ""), (CALL_ERROR, "the host failed on this call".into()));
        assert_eq!(call(h, &[0xff], ""), (CALL_ERROR, "invalid host method: not UTF-8".into()));
        if h.ctx.is_null() { RUN_ERROR } else { RUN_OK }
    }

    #[test]
    fn calls_reach_the_host_and_errors_come_back() {
        let mut e = Echo(vec![]);
        std::panic::set_hook(Box::new(|_| {}));
        // SAFETY: a test plugin
        let code = unsafe { run_with(7, plugin, &mut e) };
        let _ = std::panic::take_hook();
        assert_eq!(code, RUN_OK);
        assert_eq!(e.0, vec!["echo:{}", "fail:x", "boom:"]);
    }
}
