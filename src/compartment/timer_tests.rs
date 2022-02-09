#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::utils::Time;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_new() {
        let test = new();

        assert_eq!(test.started, false);
        assert_eq!(test.attempt, 0);
        assert_eq!(test.next_timer_time, Time::now());
    }

    #[test]
    fn test_start() {
        let mut test = new();

        test.start();

        assert_eq!(test.started, true);
        assert_eq!(test.next_timer_time, Time::now() + test.period);
    }

    #[test]
    fn test_restart() {
        let mut test = new();

        test.attempt = 1;
        test.restart();

        assert_eq!(test.started, true);
        assert_eq!(test.attempt, 0);
        assert_eq!(test.next_timer_time, Time::now() + test.period);
    }

    #[test]
    fn test_delayed_fire() {
        let mut test = new();

        test.delayed_fire();

        assert_eq!(test.started, true);
        assert_eq!(test.next_timer_time, Time::now());
    }

    #[test]
    fn test_stop() {
        let mut test = new();

        test.started = true;
        test.stop();

        assert_eq!(test.started, false);
    }

    #[test]
    fn test_is_fired() {
        let mut test = new();

        assert_eq!(test.is_fired(Time::now()), false);

        test.started = true;
        assert_eq!(test.is_fired(Time::now()), false);

        Time::advance(test.period);

        assert_eq!(test.is_fired(Time::now()), true);

        assert_eq!(test.attempt, 1);
        assert_eq!(test.next_timer_time, Time::now() + test.period);
    }

    fn new() -> Timer {
        Time::set(Instant::now());

        let period = Duration::from_secs(1);
        Timer::new(false, period)
    }
}
