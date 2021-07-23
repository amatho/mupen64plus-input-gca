use mupen64plus_input_gca::adapter::{GCAdapter, LAST_ADAPTER_STATE};
use std::time::{Duration, Instant};

fn main() {
    let adapter = GCAdapter::new().expect("could not connect to adapter");

    let started = Instant::now();

    loop {
        if started.elapsed() > Duration::from_secs(10) {
            break;
        }

        adapter.read();
        if LAST_ADAPTER_STATE.controller_state(0).any() {
            println!("{:?}", LAST_ADAPTER_STATE.controller_state(0));
        }
    }
}
