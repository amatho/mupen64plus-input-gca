#![feature(array_methods)]

#[macro_use]
mod debug;
pub mod adapter;
mod ffi;
#[macro_use]
mod static_cstr;

use adapter::ADAPTER_STATE;
use debug::M64Message;
use ffi::*;
use static_cstr::StaticCStr;
use std::{
    ffi::c_void,
    mem::ManuallyDrop,
    os::raw::{c_char, c_int, c_uchar},
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};

#[cfg(unix)]
use libloading::os::unix::Library;
#[cfg(windows)]
use libloading::os::windows::Library;

struct PluginInfo {
    name: StaticCStr,
    version: c_int,
    target_api_version: c_int,
}

static PLUGIN_INFO: PluginInfo = PluginInfo {
    name: static_cstr!("GC Adapter (for Wii U or Switch) Input Plugin"),
    version: 0x000203,            // v0.2.3
    target_api_version: 0x020100, // v2.1.0
};

static IS_INIT: AtomicBool = AtomicBool::new(false);

/// Start up the plugin.
///
/// # Safety
///
/// `core_lib_handle` cannot be null and must be a pointer to the mupen64plus-core dynamic library.
/// `debug_callback` cannot be null and must be a valid C function pointer with the correct type signature.
#[no_mangle]
pub unsafe extern "C" fn PluginStartup(
    core_lib_handle: m64p_dynlib_handle,
    context: *mut c_void,
    debug_callback: debug::DebugCallback,
) -> m64p_error {
    if IS_INIT.load(Ordering::Acquire) {
        debug_print!(M64Message::Error, "Plugin was already initialized");
        return m64p_error_M64ERR_ALREADY_INIT;
    }

    IS_INIT.store(true, Ordering::Release);

    // Register a custom panic hook in order to stop the adapter thread
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |p| {
        IS_INIT.store(false, Ordering::Release);
        default_panic(p);
    }));

    debug::init(debug_callback, context);
    debug_print!(M64Message::Info, "PluginStartup called");

    // Make sure to NOT free the library associated with the handle.
    // That would make other plugins error.
    let lib = ManuallyDrop::new(Library::from_raw(core_lib_handle.cast()));

    let core_api_version_fn = if let Ok(sym) =
        lib.get::<extern "C" fn(*mut c_int, *mut c_int, *mut c_int, *mut c_int)>(
            b"CoreGetAPIVersions\0",
        ) {
        sym
    } else {
        debug_print!(
            M64Message::Error,
            "Could not find function for getting core API versions"
        );
        return m64p_error_M64ERR_INPUT_INVALID;
    };

    let mut core_ver = 0;
    core_api_version_fn(
        &mut core_ver as *mut _,
        ptr::null_mut(),
        ptr::null_mut(),
        ptr::null_mut(),
    );

    debug_print!(
        M64Message::Info,
        "Core API reported version {:#08X}",
        core_ver
    );

    if core_ver < PLUGIN_INFO.target_api_version
        || core_ver & 0xfff0000 != PLUGIN_INFO.target_api_version & 0xfff0000
    {
        debug_print!(
            M64Message::Error,
            "Plugin is incompatible with core API version"
        );
        return m64p_error_M64ERR_INCOMPATIBLE;
    }

    if adapter::start_read_thread().is_err() {
        debug_print!(M64Message::Error, "Could not start adapter read thread");
        return m64p_error_M64ERR_PLUGIN_FAIL;
    }

    m64p_error_M64ERR_SUCCESS
}

/// Shut down the plugin.
///
/// This function is not unsafe, but if this is not called then the adapter thread will continue running.
#[no_mangle]
pub extern "C" fn PluginShutdown() -> m64p_error {
    debug_print!(M64Message::Info, "PluginShutdown called");

    IS_INIT.store(false, Ordering::Release);

    m64p_error_M64ERR_SUCCESS
}

/// Get the plugin type, version, target API version, name, and capabilities.
///
/// # Safety
///
/// The caller has to make sure the given pointers are pointing to correct types.
#[no_mangle]
pub unsafe extern "C" fn PluginGetVersion(
    plugin_type: *mut m64p_plugin_type,
    plugin_version: *mut c_int,
    api_version: *mut c_int,
    plugin_name_ptr: *mut *const c_char,
    capabilities: *mut c_int,
) -> m64p_error {
    debug_print!(M64Message::Info, "PluginGetVersion called");

    if !plugin_type.is_null() {
        *plugin_type = m64p_plugin_type_M64PLUGIN_INPUT;
    }
    if !plugin_version.is_null() {
        *plugin_version = PLUGIN_INFO.version;
    }
    if !api_version.is_null() {
        *api_version = PLUGIN_INFO.target_api_version;
    }
    if !plugin_name_ptr.is_null() {
        *plugin_name_ptr = PLUGIN_INFO.name.as_ptr();
    }
    if !capabilities.is_null() {
        *capabilities = 0;
    }

    m64p_error_M64ERR_SUCCESS
}

/// Initiate controllers by filling the given `CONTROL_INFO` struct.
///
/// # Safety
///
/// `control_info` must point to an initialized `CONTROL_INFO` struct, and the `Controls` field must point to an array
/// of length 4 with initialized `CONTROL` structs.
#[no_mangle]
pub unsafe extern "C" fn InitiateControllers(control_info: CONTROL_INFO) {
    debug_print!(M64Message::Info, "InitiateControllers called");

    let controls = control_info.Controls;
    #[cfg(feature = "m64p_compat")]
    let controls = controls as *mut CONTROL_M64P;

    for i in 0..4 {
        (*controls.add(i)).RawData = 0;
        (*controls.add(i)).Present = 1;
    }

    if !adapter::ADAPTER_STATE.any_connected() {
        debug_print!(
            M64Message::Warning,
            "No controllers connected, but hotplugging is supported"
        );
    }
}

/// Get the state of the buttons by reading from the adapter.
///
/// # Safety
///
/// `keys` must point to an intialized `BUTTONS` union.
#[no_mangle]
pub unsafe extern "C" fn GetKeys(control: c_int, keys: *mut BUTTONS) {
    read_from_adapter(control, keys);
}

/// Process the command and possibly read the controller. Currently unused, since raw data is disabled.
///
/// # Safety
///
/// `command` must be a valid u8 array with length dependent of the given command.
#[no_mangle]
pub unsafe extern "C" fn ReadController(_control: c_int, _command: *mut u8) {}

/// Currently unused, only needed to be a valid input plugin.
#[no_mangle]
pub extern "C" fn ControllerCommand(_control: c_int, _command: *mut c_uchar) {}

/// Currently unused, only needed to be a valid input plugin.
#[no_mangle]
pub extern "C" fn RomOpen() -> c_int {
    debug_print!(M64Message::Info, "RomOpen called");

    1
}

/// Currently unused, only needed to be a valid input plugin.
#[no_mangle]
pub extern "C" fn RomClosed() {
    debug_print!(M64Message::Info, "RomClosed called");
}

/// Currently unused, only needed to be a valid input plugin.
#[no_mangle]
pub extern "C" fn SDL_KeyDown(_keymod: c_int, _keysym: c_int) {
    debug_print!(M64Message::Info, "SDL_KeyDown called");
}

/// Currently unused, only needed to be a valid input plugin.
#[no_mangle]
pub extern "C" fn SDL_KeyUp(_keymod: c_int, _keysym: c_int) {
    debug_print!(M64Message::Info, "SDL_KeyUp called");
}

unsafe fn read_from_adapter(control: c_int, keys: *mut BUTTONS) {
    if !ADAPTER_STATE.is_connected(control) {
        return;
    }

    let keys = &mut *keys;
    keys.Value = 0;

    let s = ADAPTER_STATE.controller_state(control);

    const DEADZONE: u8 = 40;
    let (stick_x, stick_y) = s.stick_with_deadzone(DEADZONE);
    let (substick_x, substick_y) = s.substick_with_deadzone(DEADZONE);

    let c_left = s.y || substick_x < 0;
    let c_right = s.x || substick_x > 0;
    let c_down = substick_y < 0;
    let c_up = substick_y > 0;

    if s.right {
        keys.Value |= 0x0001;
    }
    if s.left {
        keys.Value |= 0x0002;
    }
    if s.down {
        keys.Value |= 0x0004;
    }
    if s.up {
        keys.Value |= 0x0008;
    }
    if s.start {
        keys.Value |= 0x0010;
    }
    // Use the L trigger for N64 Z
    if s.l || s.trigger_left > 148 {
        keys.Value |= 0x0020;
    }
    if s.b {
        keys.Value |= 0x0040;
    }
    if s.a {
        keys.Value |= 0x0080;
    }
    if c_right {
        keys.Value |= 0x0100;
    }
    if c_left {
        keys.Value |= 0x0200;
    }
    if c_down {
        keys.Value |= 0x0400;
    }
    if c_up {
        keys.Value |= 0x0800;
    }
    if s.r || s.trigger_right > 148 {
        keys.Value |= 0x1000;
    }
    // Use the Z button for N64 L
    if s.z {
        keys.Value |= 0x2000;
    }

    keys.__bindgen_anon_1.set_X_AXIS(stick_x as i32);
    keys.__bindgen_anon_1.set_Y_AXIS(stick_y as i32);
}
