use std::time::{Duration, Instant};

pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
    fn sleep(&self, dur: Duration);
}

pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
    fn sleep(&self, dur: Duration) {
        std::thread::sleep(dur);
    }
}
