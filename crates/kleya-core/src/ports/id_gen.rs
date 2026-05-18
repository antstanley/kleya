pub trait IdGen: Send + Sync {
    fn name(&self) -> String;
}

pub struct AdjAnimalIdGen;
impl IdGen for AdjAnimalIdGen {
    fn name(&self) -> String {
        const ADJ: &[&str] = &[
            "brave", "calm", "eager", "fancy", "gentle", "happy", "jolly", "keen", "lucky",
            "merry", "nifty", "proud", "quick", "red", "sunny", "witty",
        ];
        const ANIMAL: &[&str] = &[
            "otter", "fox", "tiger", "hawk", "lynx", "wolf", "crow", "badger", "puma", "seal",
            "kite", "owl", "whale", "squid", "heron", "koi",
        ];
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let a = ADJ[(nanos as usize) % ADJ.len()];
        let b = ANIMAL[(nanos as usize / 7) % ANIMAL.len()];
        format!("kleya-{a}-{b}")
    }
}
