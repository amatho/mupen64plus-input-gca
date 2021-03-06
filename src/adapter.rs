use crate::M64Message;
use parking_lot::Mutex;
use rusb::{DeviceHandle, GlobalContext};
use std::{
    fmt::Debug,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

const ENDPOINT_IN: u8 = 0x81;
const ENDPOINT_OUT: u8 = 0x02;
const READ_LEN: usize = 37;

static ADAPTER_READ_THREAD: AtomicBool = AtomicBool::new(false);
static LAST_INPUT_STATE: Mutex<InputState> = Mutex::new(InputState::empty());

pub fn start_read_thread() -> Result<(), &'static str> {
    let gc_adapter = if let Ok(gc) = GCAdapter::new() {
        gc
    } else {
        debug_print!(M64Message::Error, "Could not connect to GameCube adapter");
        return Err("could not initialize GameCube adapter");
    };

    ADAPTER_READ_THREAD.store(true, Ordering::Relaxed);

    thread::spawn(move || {
        debug_print!(M64Message::Info, "Adapter thread started");

        while ADAPTER_READ_THREAD.load(Ordering::Relaxed) {
            *LAST_INPUT_STATE.lock() = gc_adapter.read();

            // Gives a polling rate of approx. 1000 Hz
            thread::sleep(Duration::from_millis(1));
        }

        debug_print!(M64Message::Info, "Adapter thread stopped");
    });

    Ok(())
}

pub fn stop_read_thread() {
    ADAPTER_READ_THREAD.store(false, Ordering::Relaxed);
}

pub fn last_input_state() -> InputState {
    *LAST_INPUT_STATE.lock()
}

pub struct GCAdapter {
    handle: DeviceHandle<GlobalContext>,
}

impl Debug for GCAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "GCAdapter with product string: {}",
            self.handle
                .read_product_string_ascii(&self.handle.device().device_descriptor().unwrap())
                .unwrap()
        )
    }
}

impl GCAdapter {
    pub fn new() -> Result<Self, rusb::Error> {
        let device = rusb::devices()?
            .iter()
            .find(|dev| {
                let dev_desc = dev.device_descriptor().unwrap();
                if dev_desc.vendor_id() == 0x057E && dev_desc.product_id() == 0x0337 {
                    println!("Found GCN adapter: {:?}", dev_desc);
                    true
                } else {
                    false
                }
            })
            .ok_or(rusb::Error::NoDevice)?;

        let mut handle = device.open()?;

        if handle.kernel_driver_active(0).unwrap_or(false) {
            handle.detach_kernel_driver(0)?;
        }

        handle.claim_interface(0)?;
        handle.write_interrupt(ENDPOINT_OUT, &[0x13], Duration::from_millis(16))?;

        Ok(GCAdapter { handle })
    }

    pub fn read(&self) -> InputState {
        let mut buf = [0; READ_LEN];
        self.handle
            .read_interrupt(ENDPOINT_IN, &mut buf, Duration::from_millis(16))
            .unwrap();
        InputState::new(buf)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct InputState {
    buf: [u8; READ_LEN],
}

impl InputState {
    const fn new(buf: [u8; READ_LEN]) -> Self {
        InputState { buf }
    }

    pub const fn empty() -> Self {
        InputState { buf: [0; READ_LEN] }
    }

    /// Get the `ControllerState` for the given channel
    pub fn controller_state<T: Into<Channel>>(&self, channel: T) -> ControllerState {
        let channel = channel.into() as usize;

        if let [b1, b2, stick_x, stick_y, substick_x, substick_y, trigger_left, trigger_right, ..] =
            self.buf[(9 * channel) + 2..]
        {
            ControllerState {
                a: b1 & (1 << 0) > 0,
                b: b1 & (1 << 1) > 0,
                x: b1 & (1 << 2) > 0,
                y: b1 & (1 << 3) > 0,

                left: b1 & (1 << 4) > 0,
                right: b1 & (1 << 5) > 0,
                down: b1 & (1 << 6) > 0,
                up: b1 & (1 << 7) > 0,

                start: b2 & (1 << 0) > 0,
                z: b2 & (1 << 1) > 0,
                r: b2 & (1 << 2) > 0,
                l: b2 & (1 << 3) > 0,

                stick_x,
                stick_y,
                substick_x,
                substick_y,
                trigger_left,
                trigger_right,
            }
        } else {
            ControllerState::default()
        }
    }

    /// Check if a controller is connected to the given channel.
    pub fn is_connected<T: Into<Channel>>(&self, channel: T) -> bool {
        // 0 = No controller connected
        // 1 = Wired controller
        // 2 = Wireless controller
        let controller_type = self.buf[1 + (9 * channel.into() as usize)] >> 4;

        controller_type != 0
    }

    pub fn any_connected(&self) -> bool {
        (0..4)
            .map(|i| self.is_connected(i))
            .any(std::convert::identity)
    }
}

#[derive(Debug, Default, Copy, Clone)]
pub struct ControllerState {
    pub a: bool,
    pub b: bool,
    pub x: bool,
    pub y: bool,

    pub left: bool,
    pub right: bool,
    pub down: bool,
    pub up: bool,

    pub start: bool,
    pub z: bool,
    pub r: bool,
    pub l: bool,

    pub stick_x: u8,
    pub stick_y: u8,
    pub substick_x: u8,
    pub substick_y: u8,
    pub trigger_left: u8,
    pub trigger_right: u8,
}

impl ControllerState {
    pub fn any(&self) -> bool {
        self.a
            || self.b
            || self.x
            || self.y
            || self.start
            || self.left
            || self.right
            || self.down
            || self.up
            || self.l
            || self.r
            || self.z
            || self.stick_x < 64
            || self.stick_x > 192
            || self.stick_y < 64
            || self.stick_y > 192
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Channel {
    One = 0,
    Two,
    Three,
    Four,
}

impl From<usize> for Channel {
    fn from(x: usize) -> Self {
        match x {
            0 => Channel::One,
            1 => Channel::Two,
            2 => Channel::Three,
            _ => Channel::Four,
        }
    }
}

impl From<i32> for Channel {
    fn from(x: i32) -> Self {
        match x {
            i32::MIN..=0 => Channel::One,
            1 => Channel::Two,
            2 => Channel::Three,
            3..=i32::MAX => Channel::Four,
        }
    }
}
