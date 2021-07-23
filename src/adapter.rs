use crate::{M64Message, IS_INIT};
use rusb::{DeviceHandle, GlobalContext};
use std::{
    fmt::Debug,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};

const ENDPOINT_IN: u8 = 0x81;
const ENDPOINT_OUT: u8 = 0x02;
const READ_LEN: usize = 37;

pub static LAST_ADAPTER_STATE: AdapterState = AdapterState::new();

pub fn start_read_thread() -> Result<(), &'static str> {
    let gc_adapter = if let Ok(gc) = GCAdapter::new() {
        gc
    } else {
        debug_print!(M64Message::Error, "Could not connect to GameCube adapter");
        return Err("could not initialize GameCube adapter");
    };

    thread::spawn(move || {
        debug_print!(M64Message::Info, "Adapter thread started");

        while IS_INIT.load(Ordering::Acquire) {
            gc_adapter.read();

            // Gives a polling rate of approx. 1000 Hz
            thread::sleep(Duration::from_millis(1));
        }

        debug_print!(M64Message::Info, "Adapter thread stopped");
    });

    Ok(())
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

    pub fn read(&self) {
        let mut buf = [0u64; 5];
        let mut byte_buf = bytemuck::bytes_of_mut(&mut buf);

        self.handle
            .read_interrupt(ENDPOINT_IN, &mut byte_buf, Duration::from_millis(16))
            .unwrap();

        LAST_ADAPTER_STATE
            .buf_chunks
            .iter()
            .zip(buf.iter())
            .for_each(|(ac, &bc)| ac.store(bc, Ordering::Release));
    }
}

#[derive(Debug)]
pub struct AdapterState {
    buf_chunks: [AtomicU64; 5],
}

impl AdapterState {
    pub const fn new() -> Self {
        AdapterState {
            buf_chunks: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
        }
    }

    /// Get the `ControllerState` for the given channel
    pub fn controller_state<T: Into<Channel>>(&self, channel: T) -> ControllerState {
        let channel = channel.into() as usize;
        let buf_chunks = self
            .buf_chunks
            .each_ref()
            .map(|c| c.load(Ordering::Acquire));
        let buf = &bytemuck::bytes_of(&buf_chunks)[..READ_LEN];

        if let [b1, b2, stick_x, stick_y, substick_x, substick_y, trigger_left, trigger_right, ..] =
            buf[(9 * channel) + 2..]
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
        let buf_chunks = self
            .buf_chunks
            .each_ref()
            .map(|c| c.load(Ordering::Acquire));
        let buf = &bytemuck::bytes_of(&buf_chunks)[..READ_LEN];

        // 0 = No controller connected
        // 1 = Wired controller
        // 2 = Wireless controller
        let controller_type = buf[1 + (9 * channel.into() as usize)] >> 4;
        controller_type != 0
    }

    pub fn any_connected(&self) -> bool {
        (0..4)
            .map(|i| self.is_connected(i))
            .any(std::convert::identity)
    }
}

impl Default for AdapterState {
    fn default() -> Self {
        AdapterState::new()
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
