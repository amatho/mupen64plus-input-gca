use mupen64plus_input_gca::adapter::{AdapterState, ControllerState, GcAdapter};
use std::time::{Duration, Instant};

fn all_controller_states(state: &AdapterState) -> impl Iterator<Item = ControllerState> {
    [
        state.controller_0,
        state.controller_1,
        state.controller_2,
        state.controller_3,
    ]
    .into_iter()
}

fn any(state: ControllerState) -> bool {
    const CONTROL_DEADZONE: u8 = 30;
    const C_DEADZONE: u8 = 30;
    const TRIGGER_THRESHOLD: u8 = 168;
    state.is_connected()
        && (state.a
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
            || state.stick_x < 128 - CONTROL_DEADZONE
            || state.stick_x > 128 + CONTROL_DEADZONE
            || state.stick_y < 128 - CONTROL_DEADZONE
            || state.stick_y > 128 + CONTROL_DEADZONE
            || state.substick_x < 128 - C_DEADZONE
            || state.substick_x > 128 + C_DEADZONE
            || state.substick_y < 128 - C_DEADZONE
            || state.substick_y > 128 + C_DEADZONE)
}

#[test]
fn receives_input() {
    const ERR: &str = "make sure the adapter is connected, and press the input(s) you want to test";

    let adapter = GcAdapter::new().expect(ERR);
    let started = Instant::now();

    let state = AdapterState::from(adapter.read().unwrap());

    if !state.any_connected() {
        eprintln!("no controllers detected, but might be a false negative");
    }

    let mut any_input = false;
    loop {
        if started.elapsed() > Duration::from_secs(10) {
            break;
        }

        let buf = adapter.read().unwrap();
        let state = AdapterState::from(buf);
        for (i, s) in (0..4)
            .zip(all_controller_states(&state))
            .filter(|(_, s)| any(*s))
        {
            any_input = true;
            println!("Channel {i}: {s:?}");
        }
    }

    assert!(any_input);
}
