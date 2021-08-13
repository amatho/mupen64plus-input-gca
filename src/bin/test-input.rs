use mupen64plus_input_gca::adapter::{AdapterState, ControllerState, GCAdapter};
use std::{
    io::Write,
    time::{Duration, Instant},
};

fn any(state: ControllerState) -> bool {
    const CONTROL_DEADZONE: u8 = 15;
    const CONTROL_SENSITIVITY: u8 = 100;
    const C_DEADZONE: u8 = 15;
    const TRIGGER_THRESHOLD: u8 = 168;
    let (stick_x, stick_y) = state.stick_with_deadzone(CONTROL_DEADZONE, CONTROL_SENSITIVITY);
    let (substick_x, substick_y) = state.substick_with_deadzone(C_DEADZONE);
    state.a
        || state.b
        || state.x
        || state.y
        || state.start
        || state.left
        || state.right
        || state.down
        || state.up
        || state.l
        || state.trigger_left > TRIGGER_THRESHOLD
        || state.r
        || state.trigger_right > TRIGGER_THRESHOLD
        || state.z
        || stick_x != 0
        || stick_y != 0
        || substick_x != 0
        || substick_y != 0
}

fn main() {
    let adapter = GCAdapter::new().expect("could not connect to adapter");
    let started = Instant::now();

    print!("Choose channel to read from [0-3]: ");
    std::io::stdout().flush().unwrap();

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    let chan: usize = input[0..1].parse().expect("channel must be a number");

    let mut state = AdapterState::new();
    state.set_buf(adapter.read());

    if !state.is_connected(chan) {
        eprintln!("WARNING: no controller connected to the specified channel")
    }

    loop {
        if started.elapsed() > Duration::from_secs(10) {
            break;
        }

        state.set_buf(adapter.read());
        if any(state.controller_state(chan)) {
            println!("Channel {}: {:?}", chan, state.controller_state(chan));
        }
    }
}
