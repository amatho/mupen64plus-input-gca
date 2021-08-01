use mupen64plus_input_gca::adapter::{AdapterState, GCAdapter};
use std::{
    io::Write,
    time::{Duration, Instant},
};

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
        if state.controller_state(chan).any() {
            println!("Channel {}: {:?}", chan, state.controller_state(chan));
        }
    }
}
