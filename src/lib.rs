#![feature(array_methods)]

#[macro_use]
mod debug;
pub mod adapter;
mod ffi;
#[macro_use]
mod static_cstr;

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
    version: 0x000202,            // v0.2.2
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

/// Get the plugin version and etc.
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

/// Currently unused, only needed to be a valid input plugin.
#[no_mangle]
pub extern "C" fn ControllerCommand(control: c_int, _command: *mut c_uchar) {
    if control == -1 {
        return;
    }

    debug_print!(
        M64Message::Info,
        "ControllerCommand called (control = {})",
        control
    );
}

/// Get which keys are pressed.
///
/// This is currently unused, as it seems like only raw data works (using `ReadController` and `ControllerCommand`).
///
/// # Safety
///
/// `keys` must point to an intialized `BUTTONS` union.
#[no_mangle]
pub unsafe extern "C" fn GetKeys(control: c_int, keys: *mut BUTTONS) {
    debug_print!(
        M64Message::Info,
        "GetKeys called with control = {}",
        control
    );

    read_from_adapter(control, keys);
}

/// Fills the given `CONTROL_INFO` struct.
///
/// # Safety
///
/// `control_info` must point to an initialized `CONTROL_INFO` struct, and the `Controls` field must point to an array
/// of length 4 with initialized `CONTROL` structs.
#[no_mangle]
pub unsafe extern "C" fn InitiateControllers(control_info: CONTROL_INFO) {
    debug_print!(M64Message::Info, "InitiateControllers called");

    let controls = control_info.Controls as *mut [CONTROL; 4];

    for i in 0..4 {
        (*controls)[i].RawData = 1;
        (*controls)[i].Present = 1;
    }

    if !adapter::LAST_ADAPTER_STATE.any_connected() {
        debug_print!(
            M64Message::Warning,
            "No controllers connected, but hotplugging is supported"
        );
    }
}

/// Process the command and possibly read the controller.
///
/// # Safety
///
/// `command` must be a valid u8 array with length dependent of the given command.
#[no_mangle]
pub unsafe extern "C" fn ReadController(control: c_int, command: *mut u8) {
    if control == -1 {
        return;
    }

    let cmd = ReadCommand::from(*command.add(2));
    match cmd {
        ReadCommand::GetStatus | ReadCommand::ResetController => {
            *command.add(3) = 0x04 | 0x01; // RD_GAMEPAD | RD_ABSOLUTE
            *command.add(4) = 0x00; // RD_NOEEPROM
            *command.add(5) = 0x02; // RD_NOPLUGIN | RD_NOTINITIALIZED
        }
        ReadCommand::ReadKeys => {
            let mut buttons = BUTTONS { Value: 0 };

            read_from_adapter(control, &mut buttons as *mut _);

            *(command.add(3) as *mut u32) = buttons.Value;
        }
        ReadCommand::ReadEepRom => {}
        ReadCommand::WriteEepRom => {}
        ReadCommand::Unrecognized => {
            let c1 = *command.add(1);
            *command.add(1) = c1 | 0x80; // 0x80 = RD_ERROR
        }
    }
}

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

enum ReadCommand {
    GetStatus,
    ReadKeys,
    ResetController,
    ReadEepRom,
    WriteEepRom,

    Unrecognized,
}

impl From<u8> for ReadCommand {
    fn from(x: u8) -> Self {
        match x {
            0x00 => ReadCommand::GetStatus,
            0x01 => ReadCommand::ReadKeys,
            0xff => ReadCommand::ResetController,
            0x04 => ReadCommand::ReadEepRom,
            0x05 => ReadCommand::WriteEepRom,
            _ => ReadCommand::Unrecognized,
        }
    }
}

unsafe fn read_from_adapter(control: c_int, keys: *mut BUTTONS) {
    let adapter_state = &adapter::LAST_ADAPTER_STATE;

    if !adapter_state.is_connected(control) {
        return;
    }

    let keys = &mut *keys;

    let s = adapter_state.controller_state(control as usize);

    let c_left = s.y || s.substick_x < 88;
    let c_right = s.x || s.substick_x > 168;
    let c_down = s.substick_y < 88;
    let c_up = s.substick_y > 168;

    const DEADZONE: i32 = 40;
    let (stick_x, stick_y) = {
        let x = s.stick_x.wrapping_add(128) as i8 as i32;
        let y = s.stick_y.wrapping_add(128) as i8 as i32;

        let pos = x.pow(2) + y.pow(2);
        if pos < DEADZONE.pow(2) {
            (0, 0)
        } else {
            (x, y)
        }
    };

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
