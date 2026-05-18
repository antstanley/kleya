use crate::ports::id_gen::IdGen;
use std::sync::Mutex;

pub struct FakeIdGen {
    next: Mutex<u64>,
}

impl FakeIdGen {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next: Mutex::new(0),
        }
    }
}

impl Default for FakeIdGen {
    fn default() -> Self {
        Self::new()
    }
}

impl IdGen for FakeIdGen {
    fn name(&self) -> String {
        let mut g = self.next.lock().expect("mutex");
        let v = *g;
        *g += 1;
        format!("kleya-test-{v:04}")
    }
}
