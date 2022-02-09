#[cfg(test)]
pub use utils::add_track;
#[cfg(test)]
pub use utils::expect_tracks;
#[cfg(test)]
pub use utils::init_tracks;

pub use utils::Time;

#[cfg(not(test))]
pub mod utils {
    use std::time::Instant;

    pub struct Time {}

    impl Time {
        pub fn now() -> Instant {
            Instant::now()
        }
    }

    #[macro_export]
    macro_rules! track_method {
        ($($arg:tt)*) => {};
    }

    #[macro_export]
    macro_rules! track {
        ($($arg:tt)*) => {};
    }
}

#[cfg(test)]
pub mod utils {
    use pretty_assertions::assert_eq;
    use std::cell::Cell;
    use std::cell::RefCell;
    use std::time::{Duration, Instant};

    thread_local! {
        static LOCAL_TIME: Cell<Instant> = Cell::new(Instant::now());
    }

    pub struct Time {}

    impl Time {
        pub fn now() -> Instant {
            LOCAL_TIME.with(|t| t.get())
        }

        pub fn set(time: Instant) {
            LOCAL_TIME.with(|t| {
                t.set(time);
            });
        }

        pub fn advance(duration: Duration) {
            LOCAL_TIME.with(|t| {
                t.set(t.get() + duration);
            });
        }
    }

    thread_local! {
        static METHOD_TRACKS: RefCell<Vec<String>> = RefCell::new(vec![]);
    }

    pub fn init_tracks() {
        METHOD_TRACKS.with(|t| {
            t.borrow_mut().clear();
        });
    }

    pub fn add_track(track: String) {
        METHOD_TRACKS.with(|t| {
            t.borrow_mut().push(track);
        });
    }

    pub fn expect_tracks(tracks: Vec<&str>) {
        METHOD_TRACKS.with(|t| {
            assert_eq!(*t.borrow(), tracks);
        });
    }

    #[macro_export]
    macro_rules! track_method {
        ($var:ident, $method_name:expr) => {
            let $var: &str = $method_name;
        };
    }

    #[macro_export]
    macro_rules! track {
        ($var:ident, $($args:tt)*) => {
            let track = format!("{}.{}", $var, format!($($args)*));
            crate::common::utils::add_track(track);
        };
    }
}
