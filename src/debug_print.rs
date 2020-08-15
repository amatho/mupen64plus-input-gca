macro_rules! debug_print {
    ($level:expr, $s:expr) => {
        debug_print!($level, $s,)
    };
    ($level:expr, $s:expr, $($arg:expr),*) => {{
        if cfg!(debug_assertions) {
            if let Some(di) = $crate::DEBUG_INFO.get() {
                let context = di.context_ptr.load(::std::sync::atomic::Ordering::Relaxed);
                if !context.is_null() {
                    let message = ::std::ffi::CString::new(format!($s $(, $arg)*)).unwrap();
                    (di.callback)(context, $level as ::std::os::raw::c_int, message.as_ptr());
                }
            }
        }
    }};
}
