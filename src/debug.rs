use crate::M64Message;
use once_cell::sync::OnceCell;
use std::{
    ffi::{c_void, CString},
    os::raw::{c_char, c_int},
    sync::atomic::AtomicPtr,
};

pub fn init(
    debug_callback: extern "C" fn(*mut c_void, c_int, *const c_char),
    context_ptr: *mut c_void,
) -> Result<(), &'static str> {
    DEBUG_INFO
        .set(DebugInfo::new(debug_callback, context_ptr))
        .map_err(|_| "debug info was already initialized")
}

pub static DEBUG_INFO: OnceCell<DebugInfo> = OnceCell::new();

macro_rules! debug_print {
    ($level:expr, $s:expr) => {
        debug_print!($level, $s,)
    };
    ($level:expr, $s:expr, $($arg:expr),*) => {{
        if cfg!(debug_assertions) {
            $crate::debug::__print_debug_message($level, format!($s $(, $arg)*));
        }
    }};
}

#[doc(hidden)]
pub(crate) fn __print_debug_message(level: M64Message, message: String) {
    if let Some(di) = DEBUG_INFO.get() {
        let context = di.context_ptr.load(::std::sync::atomic::Ordering::Relaxed);
        if !context.is_null() {
            let message = CString::new(message).unwrap();
            (di.callback)(context, level as c_int, message.as_ptr());
        }
    }
}

#[derive(Debug)]
pub struct DebugInfo {
    callback: extern "C" fn(*mut c_void, c_int, *const c_char),
    context_ptr: AtomicPtr<c_void>,
}

impl DebugInfo {
    fn new(
        debug_callback: extern "C" fn(*mut c_void, c_int, *const c_char),
        context: *mut c_void,
    ) -> Self {
        Self {
            callback: debug_callback,
            context_ptr: AtomicPtr::new(context),
        }
    }
}
