use mupen64plus_input_gca::adapter::GCAdapter;
use std::time::{Duration, Instant};

fn main() {
    let adapter = GCAdapter::new().expect("could not connect to adapter");

    let started = Instant::now();

    loop {
        if started.elapsed() > Duration::from_secs(10) {
            break;
        }

        let state = adapter.read().controller_state(0);
        if state.any() {
            println!("{:?}", adapter.read().controller_state(0));
        }
    }
}
