use crate::{M64Message, IS_INIT};
use parking_lot::Mutex;
use rusb::{DeviceHandle, GlobalContext};
use std::{
    convert::{TryFrom, TryInto},
    fmt::Debug,
    sync::atomic::Ordering,
    thread,
    time::Duration,
};

const ENDPOINT_IN: u8 = 0x81;
const ENDPOINT_OUT: u8 = 0x02;
const READ_LEN: usize = 37;

pub static ADAPTER_STATE: AdapterState = AdapterState::new();

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
            *ADAPTER_STATE.buf.lock() = gc_adapter.read();

            // Gives a polling rate of approx. 1000 Hz
            thread::park_timeout(Duration::from_millis(1));
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
                dev_desc.vendor_id() == 0x057E && dev_desc.product_id() == 0x0337
            })
            .ok_or(rusb::Error::NoDevice)?;

        let mut handle = device.open()?;

        if handle.kernel_driver_active(0).unwrap_or(false) {
            handle.detach_kernel_driver(0)?;
        }

        // From Dolphin emulator source:
        // "This call makes Nyko-brand (and perhaps other) adapters work.
        // However it returns LIBUSB_ERROR_PIPE with Mayflash adapters."
        let res = handle.write_control(0x21, 11, 0x0001, 0, &[], Duration::from_millis(1000));
        if let Err(e) = res {
            debug_print!(
                M64Message::Warning,
                "Control transfer failed with error: {:?}",
                e
            );
        }

        handle.claim_interface(0)?;
        handle.write_interrupt(ENDPOINT_OUT, &[0x13], Duration::from_millis(16))?;

        Ok(GCAdapter { handle })
    }

    pub fn read(&self) -> [u8; READ_LEN] {
        let mut buf = [0; READ_LEN];

        match self
            .handle
            .read_interrupt(ENDPOINT_IN, &mut buf, Duration::from_millis(16))
        {
            Ok(_) | Err(rusb::Error::Timeout) => buf,
            Err(e) => panic!("error while reading from adapter: {:?}", e),
        }
    }
}

#[derive(Debug)]
pub struct AdapterState {
    buf: Mutex<[u8; 37]>,
}

impl AdapterState {
    pub const fn new() -> Self {
        AdapterState {
            buf: Mutex::new([0; READ_LEN]),
        }
    }

    /// Get the `ControllerState` for the given channel
    pub fn controller_state<T>(&self, channel: T) -> ControllerState
    where
        T: TryInto<Channel>,
        <T as TryInto<Channel>>::Error: Debug,
    {
        let channel = channel.try_into().unwrap() as usize;
        let buf = *self.buf.lock();

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
            debug_print!(
                M64Message::Error,
                "Entered unreachable code (invalid adapter buffer)"
            );
            ControllerState::default()
        }
    }

    /// Check if a controller is connected to the given channel.
    pub fn is_connected<T>(&self, channel: T) -> bool
    where
        T: TryInto<Channel>,
        <T as TryInto<Channel>>::Error: Debug,
    {
        let buf = *self.buf.lock();
        let channel = channel.try_into().unwrap();

        // 0 = No controller connected
        // 1 = Wired controller
        // 2 = Wireless controller
        let controller_type = buf[1 + (9 * channel as usize)] >> 4;
        controller_type != 0
    }

    pub fn any_connected(&self) -> bool {
        (0..4)
            .map(|i| self.is_connected(i))
            .any(std::convert::identity)
    }

    pub fn set_buf(&mut self, buf: [u8; 37]) {
        *self.buf.get_mut() = buf;
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
        let (stick_x, stick_y) = self.stick_with_deadzone(40);
        let (substick_x, substick_y) = self.substick_with_deadzone(40);
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
            || stick_x != 0
            || stick_y != 0
            || substick_x != 0
            || substick_y != 0
    }

    pub fn stick_with_deadzone(&self, deadzone: u8) -> (i8, i8) {
        Self::deadzoned_stick(self.stick_x, self.stick_y, deadzone)
    }

    pub fn substick_with_deadzone(&self, deadzone: u8) -> (i8, i8) {
        Self::deadzoned_stick(self.substick_x, self.substick_y, deadzone)
    }

    fn deadzoned_stick(x: u8, y: u8, deadzone: u8) -> (i8, i8) {
        let x = x.wrapping_add(128) as i8;
        let y = y.wrapping_add(128) as i8;

        let pos = (x as i32).pow(2) + (y as i32).pow(2);
        if pos < (deadzone as i32).pow(2) {
            (0, 0)
        } else {
            (x, y)
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Channel {
    One = 0,
    Two = 1,
    Three = 2,
    Four = 3,
}

impl TryFrom<usize> for Channel {
    type Error = usize;

    fn try_from(val: usize) -> Result<Self, Self::Error> {
        match val {
            0 => Ok(Channel::One),
            1 => Ok(Channel::Two),
            2 => Ok(Channel::Three),
            3 => Ok(Channel::Four),
            x => Err(x),
        }
    }
}

impl TryFrom<i32> for Channel {
    type Error = i32;

    fn try_from(val: i32) -> Result<Self, Self::Error> {
        match val {
            0 => Ok(Channel::One),
            1 => Ok(Channel::Two),
            2 => Ok(Channel::Three),
            3 => Ok(Channel::Four),
            x => Err(x),
        }
    }
}
