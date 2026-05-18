use crate::ports::clock::Clock;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct FakeClock {
    state: Mutex<(Instant, Vec<Duration>)>,
}

impl FakeClock {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Mutex::new((Instant::now(), vec![])),
        }
    }
    pub fn advance(&self, by: Duration) {
        let mut s = self.state.lock().expect("mutex");
        s.0 += by;
    }
    #[must_use]
    pub fn slept(&self) -> Vec<Duration> {
        self.state.lock().expect("mutex").1.clone()
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        self.state.lock().expect("mutex").0
    }
    fn sleep(&self, dur: Duration) {
        let mut s = self.state.lock().expect("mutex");
        s.0 += dur;
        s.1.push(dur);
    }
}
