use rusb::{DeviceHandle, GlobalContext};
use std::{convert::TryFrom, fmt::Debug, time::Duration};

const ENDPOINT_IN: u8 = 0x81;
const ENDPOINT_OUT: u8 = 0x02;
const READ_LEN: usize = 37;

pub struct GcAdapter {
    handle: DeviceHandle<GlobalContext>,
}

impl Debug for GcAdapter {
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

impl GcAdapter {
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
        let _ = handle.write_control(0x21, 11, 0x0001, 0, &[], Duration::from_millis(1000));

        handle.claim_interface(0)?;
        handle.write_interrupt(ENDPOINT_OUT, &[0x13], Duration::from_millis(16))?;

        Ok(GcAdapter { handle })
    }

    pub fn read(&self) -> rusb::Result<[u8; READ_LEN]> {
        let mut buf = [0; READ_LEN];

        match self
            .handle
            .read_interrupt(ENDPOINT_IN, &mut buf, Duration::from_millis(16))
        {
            Ok(_) | Err(rusb::Error::Timeout) => Ok(buf),
            Err(e) => Err(e),
        }
    }

    pub fn set_rumble(&self, strengths: [u8; 4]) -> rusb::Result<()> {
        let data = [0x11, strengths[0], strengths[1], strengths[2], strengths[3]];
        self.handle
            .write_interrupt(ENDPOINT_OUT, &data, Duration::from_millis(16))?;
        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub struct AdapterState {
    pub controller_0: ControllerState,
    pub controller_1: ControllerState,
    pub controller_2: ControllerState,
    pub controller_3: ControllerState,
}

impl AdapterState {
    pub const fn new() -> Self {
        Self {
            controller_0: ControllerState::new(),
            controller_1: ControllerState::new(),
            controller_2: ControllerState::new(),
            controller_3: ControllerState::new(),
        }
    }

    /// Get the `ControllerState` for the given channel
    pub fn controller_state(&self, channel: Channel) -> ControllerState {
        match channel {
            Channel::One => self.controller_0,
            Channel::Two => self.controller_1,
            Channel::Three => self.controller_2,
            Channel::Four => self.controller_3,
        }
    }

    pub fn any_connected(&self) -> bool {
        self.controller_0.is_connected()
            || self.controller_1.is_connected()
            || self.controller_2.is_connected()
            || self.controller_3.is_connected()
    }
}

impl From<[u8; READ_LEN]> for AdapterState {
    fn from(bytes: [u8; READ_LEN]) -> Self {
        let controller_0 = ControllerState::from(&bytes[1..]);
        let controller_1 = ControllerState::from(&bytes[10..]);
        let controller_2 = ControllerState::from(&bytes[19..]);
        let controller_3 = ControllerState::from(&bytes[28..]);

        Self {
            controller_0,
            controller_1,
            controller_2,
            controller_3,
        }
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct ControllerState {
    pub status: u8,

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
    pub const fn new() -> Self {
        Self {
            status: 0,
            a: false,
            b: false,
            x: false,
            y: false,
            left: false,
            right: false,
            down: false,
            up: false,
            start: false,
            z: false,
            r: false,
            l: false,
            stick_x: 0,
            stick_y: 0,
            substick_x: 0,
            substick_y: 0,
            trigger_left: 0,
            trigger_right: 0,
        }
    }

    pub fn stick_with_deadzone(&self, deadzone: u8, sensitivity: u8) -> (i8, i8) {
        const STICK_MAX: i32 = i8::MAX as i32;

        let x = self.stick_x.wrapping_add(128) as i8;
        let y = self.stick_y.wrapping_add(128) as i8;

        // Convert cartesian coordinates to polar coordinates (radius)
        let radius = ((x as f32).powi(2) + (y as f32).powi(2)).sqrt();

        if radius <= deadzone as f32 {
            return (0, 0);
        }

        // Convert cartesian coordinates to polar coordinates (angle)
        let angle = (y as f32).atan2(x as f32);

        let deadzone = deadzone as i32;
        // User-facing sensitivity is inverted (so that higher values give higher radius)
        let sensitivity = u8::MAX as i32 - sensitivity as i32;

        // Scale radius to counteract the deadzone, and fit the radius to the range [-80, 80] (N64
        // stick range).
        // This formula is a simplified version of the following:
        //
        // let radius = (radius - deadzone as f32) * (STICK_MAX as f32 / (STICK_MAX - deadzone) as f32);
        // let radius = radius * 80.0 / (STICK_MAX as f32 * (sensitivity as f32 / 100.0)) as f32;
        let radius =
            8000.0 * (radius - deadzone as f32) / (sensitivity * (STICK_MAX - deadzone)) as f32;

        // Convert back to cartesian coordinates
        let x = (radius * angle.cos()).round() as i8;
        let y = (radius * angle.sin()).round() as i8;

        (x, y)
    }

    pub fn substick_with_deadzone(&self, deadzone: u8) -> (i8, i8) {
        let x = self.substick_x.wrapping_add(128) as i8;
        let y = self.substick_y.wrapping_add(128) as i8;

        let x = if x.unsigned_abs() < deadzone { 0 } else { x };

        let y = if y.unsigned_abs() < deadzone { 0 } else { y };

        (x, y)
    }

    pub fn is_connected(&self) -> bool {
        // 0x10 = Normal
        // 0x20 = Wavebird
        (self.status & 0x10) > 0 || (self.status & 0x20) > 0
    }
}

impl From<&[u8]> for ControllerState {
    fn from(bytes: &[u8]) -> Self {
        let [status, b1, b2, stick_x, stick_y, substick_x, substick_y, trigger_left, trigger_right, ..] = *bytes else {
            panic!("invalid controller state bytes");
        };

        Self {
            status,

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_state() {
        let data: [u8; READ_LEN] = [
            0x0, 0x1, 0b10010110, 0b11110110, 0x5, 0x6, 0x7, 0x8, 0x9, 0xA, 0x2, 0b10010110,
            0b11110110, 0x5, 0x6, 0x7, 0x8, 0x9, 0xA, 0x3, 0b10010110, 0b11110110, 0x5, 0x6, 0x7,
            0x8, 0x9, 0xA, 0x4, 0b10010110, 0b11110110, 0x5, 0x6, 0x7, 0x8, 0x9, 0xA,
        ];
        let state = AdapterState::from(data);
        assert_eq!(0x1, state.controller_0.status);
        assert!(!state.controller_0.a);
        assert!(state.controller_0.b);
        assert!(state.controller_0.x);
        assert!(!state.controller_0.y);
        assert!(state.controller_0.up);
        assert!(!state.controller_0.down);
        assert!(!state.controller_0.right);
        assert!(state.controller_0.left);
        assert!(!state.controller_0.start);
        assert!(state.controller_0.z);
        assert!(state.controller_0.r);
        assert!(!state.controller_0.l);
        assert_eq!(0x2, state.controller_1.status);
        assert_eq!(0x3, state.controller_2.status);
        assert_eq!(0x4, state.controller_3.status);
        // TODO: Write more assertions
    }
}
