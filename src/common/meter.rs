use std::time::Instant;

pub struct Meter {
    name: String,
    start: Instant,
}

impl Meter {
    pub fn new(name: &str) -> Meter {
        Meter {
            name: String::from(name),
            start: Instant::now(),
        }
    }
}

impl Drop for Meter {
    fn drop(&mut self) {
        let delta = Instant::now() - self.start;
        println!("{} - {:?}", self.name, delta);
    }
}
