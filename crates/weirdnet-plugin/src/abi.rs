//! The Weirdnet plugin C ABI, version 2. Shared verbatim by hosts and plugins.
//!
//! Version 2 has the same layout and calling convention as version 1 of the SDK this one
//! descends from; it differs in its symbol names and version number, so a host knows which
//! SDK a library was built against and never mistakes one for the other.
//!
//! A plugin is a shared library (`.so`, `.dylib`, `.dll`) exporting three symbols:
//!
//! | symbol | signature | meaning |
//! |--------|-----------|---------|
//! | `weirdnet_plugin_abi` | `extern "C" fn() -> u32` | the ABI version it was built for ([`ABI_VERSION`]) |
//! | `weirdnet_plugin_info` | `extern "C" fn() -> WnStr` | static UTF-8 JSON: `{"sdk": "0.1.0", "lang": "rust"}` (and `"kind"` from `plugin!(run, kind = "…")`) |
//! | `weirdnet_plugin_run` | `unsafe extern "C" fn(*const WnHost) -> i32` | run once; [`RUN_OK`] or an error code |
//!
//! Everything else goes through the host table [`WnHost`]: `call(ctx, method, request, out)`
//! takes a method name and a JSON request and fills `out` with a JSON reply that the plugin
//! returns with `free`. Memory never crosses allocators: the host frees what it allocates,
//! and the plugin only lends (`WnStr`) what it owns.
//!
//! Compatibility: the layout of these types and the three symbols never change within an
//! ABI version. New host methods may be added at any time (`methods` lists them); a plugin
//! asking for a method the host does not know gets an error, never a crash. A host refuses
//! plugins whose `weirdnet_plugin_abi` is not one it supports.
use core::ffi::c_void;

/// The ABI version this file describes.
pub const ABI_VERSION: u32 = 2;

/// `weirdnet_plugin_run` finished; the result (or error) was handed to the host.
pub const RUN_OK: i32 = 0;
/// The run failed with an error (sent to the host with the `error` method).
pub const RUN_ERROR: i32 = 1;
/// The plugin panicked (the panic was caught; its message was sent with `error`).
pub const RUN_PANIC: i32 = 2;

/// `call` succeeded and `out` holds the JSON reply.
pub const CALL_OK: i32 = 0;
/// `call` failed and `out` holds the error message (UTF-8, not JSON).
pub const CALL_ERROR: i32 = 1;

/// Exported symbol names (NUL-terminated for `dlsym`).
pub const SYM_ABI: &[u8] = b"weirdnet_plugin_abi\0";
pub const SYM_INFO: &[u8] = b"weirdnet_plugin_info\0";
pub const SYM_RUN: &[u8] = b"weirdnet_plugin_run\0";

/// Borrowed UTF-8 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WnStr {
    pub ptr: *const u8,
    pub len: usize,
}

impl WnStr {
    pub fn new(s: &str) -> Self {
        WnStr { ptr: s.as_ptr(), len: s.len() }
    }
    /// # Safety
    /// `ptr` must point at `len` readable bytes for the lifetime `'a`.
    pub unsafe fn as_bytes<'a>(&self) -> &'a [u8] {
        if self.ptr.is_null() || self.len == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

/// Bytes owned by the host; give them back with [`WnHost::free`].
#[repr(C)]
pub struct WnBuf {
    pub ptr: *mut u8,
    pub len: usize,
    pub cap: usize,
}

impl WnBuf {
    pub const EMPTY: WnBuf = WnBuf { ptr: core::ptr::null_mut(), len: 0, cap: 0 };
}

/// Calls a host method: `method` name, `request` JSON; fills `out`; returns [`CALL_OK`] or [`CALL_ERROR`].
pub type HostCall = unsafe extern "C" fn(ctx: *mut c_void, method: WnStr, request: WnStr, out: *mut WnBuf) -> i32;
/// Frees a buffer the host filled.
pub type HostFree = unsafe extern "C" fn(ctx: *mut c_void, buf: WnBuf);

/// What the host gives a plugin for one run.
#[repr(C)]
pub struct WnHost {
    pub abi_version: u32,
    pub ctx: *mut c_void,
    pub call: HostCall,
    pub free: HostFree,
}

pub type PluginAbi = extern "C" fn() -> u32;
pub type PluginInfo = extern "C" fn() -> WnStr;
pub type PluginRun = unsafe extern "C" fn(host: *const WnHost) -> i32;
