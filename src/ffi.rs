#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(clippy::all)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(feature = "m64p_compat")]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CONTROL_M64P {
    pub Present: std::os::raw::c_int,
    pub RawData: std::os::raw::c_int,
    pub Plugin: std::os::raw::c_int,
    pub Type: std::os::raw::c_int,
}
