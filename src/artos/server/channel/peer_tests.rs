#[cfg(test)]
mod tests {
    use super::*;
    use crate::artos::common::api::MockMessageSender;
    use crate::artos::common::ErrorKind;
    use crate::artos::server::test::{MessageSenderMockBuilder, Setup};
    use crate::common::utils::expect_tracks;
    use crate::common::utils::init_tracks;
    use crate::common::utils::Time;
    use pretty_assertions::assert_eq;
    use std::time::Duration;

    impl ServerPeer {
        pub fn set_message_sender_mock(&self) {
            let mut state = self.state.borrow_mut();
            state.message_sender = Some(Box::new(MockMessageSender::new()));
        }

        pub fn set_request_in_progress(&self, value: bool) {
            let mut state = self.state.borrow_mut();
            state.in_progress = value;
        }

        pub fn set_next_heartbeat_time(&self, value: Instant) {
            let mut state = self.state.borrow_mut();
            state.next_heartbeat_time = value;
        }

        pub fn set_heartbeat_enabled(&self, value: bool) {
            let mut state = self.state.borrow_mut();
            state.heartbeat_enabled = value;
        }
    }

    #[test]
    fn test_new() {
        let test = new();
        let state = test.state.borrow();

        assert_eq!(state.heartbeat_enabled, false);
        assert_eq!(state.next_heartbeat_time, Time::now());
        assert_eq!(state.next_log_index, 1);
        assert_eq!(state.matched_index, 0);
        assert_eq!(state.rewind_step, 1);
        assert_eq!(state.last_response_time, Time::now());
        assert_eq!(state.in_progress, false);
        assert_eq!(state.started, false);
    }

    #[test]
    fn test_start() {
        let test = new();

        Time::advance(Duration::from_secs(1));

        test.start();

        let state = test.state.borrow();
        assert_eq!(state.started, true);
        assert_eq!(state.heartbeat_enabled, true);
        assert_eq!(state.last_response_time, Time::now());
        expect_tracks(vec![
            "resume_heartbeating.end",
            "update_next_heartbeat_time.end",
            "start.end",
        ]);
    }

    #[test]
    fn test_stop() {
        let test = new();

        test.start();
        init_tracks();

        test.stop();

        let state = test.state.borrow();
        assert_eq!(state.started, false);
        assert_eq!(state.heartbeat_enabled, false);
        expect_tracks(vec!["on_peer_disconnected.already_disconnected", "stop.end"]);
    }

    #[test]
    fn test_can_heartbeat_peer_timeout() {
        let mut setup = Setup::new();
        setup
            .message_sender_factory
            .create_sender(Result::Ok(MessageSenderMockBuilder::new().build()));
        let test = new_with_setup(setup);

        test.start();
        init_tracks();
        {
            let mut state = test.state.borrow_mut();
            state.peer_timeout_timer.start();
        }

        Time::advance(Duration::from_secs(10));
        test.can_heartbeat(Time::now());

        expect_tracks(vec![
            "on_peer_disconnected.already_disconnected",
            "handle_peer_timeout.end",
            "ensure_message_sender.end",
            "on_peer_connected.end",
            "can_heartbeat.end",
        ]);
    }

    #[test]
    fn test_can_heartbeat_no() {
        let test = new();

        test.start();
        init_tracks();

        test.can_heartbeat(Time::now());

        expect_tracks(vec!["can_heartbeat.end_false"]);
    }

    #[test]
    fn test_can_heartbeat() {
        let mut setup = Setup::new();
        setup
            .message_sender_factory
            .create_sender(Result::Ok(MessageSenderMockBuilder::new().build()));
        let test = new_with_setup(setup);

        test.start();
        init_tracks();
        Time::advance(Duration::from_secs(1));

        test.can_heartbeat(Time::now());

        expect_tracks(vec![
            "ensure_message_sender.end",
            "on_peer_connected.end",
            "can_heartbeat.end",
        ]);
    }

    #[test]
    fn test_send() {
        let mut setup = Setup::new();
        setup
            .message_sender_factory
            .create_sender(Result::Ok(MessageSenderMockBuilder::new().send().build()));
        let request = setup.vote_request.build();
        let test = new_with_setup(setup);

        test.start();
        init_tracks();

        test.send(Request::Vote(request));

        let state = test.state.borrow();
        assert_eq!(state.in_progress, true);
        expect_tracks(vec![
            "update_next_heartbeat_time.end",
            "ensure_message_sender.end",
            "on_peer_connected.end",
            "send.send",
            "send.end",
        ]);
    }

    #[test]
    fn test_handle_succeeded_response() {
        let test = new();

        test.start();
        init_tracks();

        {
            let mut state = test.state.borrow_mut();
            state.in_progress = true;
        }
        Time::advance(Duration::from_secs(1));

        test.handle_succeeded_response();

        let state = test.state.borrow();
        assert_eq!(state.in_progress, false);
        assert_eq!(state.last_response_time, Time::now());
        assert_eq!(state.peer_timeout_timer.is_started(), true);
        expect_tracks(vec!["resume_heartbeating.end", "handle_succeeded_response.end"]);
    }

    #[test]
    fn test_handle_failed_response() {
        let test = new();

        test.start();
        init_tracks();

        {
            let mut state = test.state.borrow_mut();
            state.in_progress = true;
        }
        Time::advance(Duration::from_secs(1));

        test.handle_failed_response();

        let state = test.state.borrow();
        assert_eq!(state.in_progress, false);
        assert_eq!(state.last_response_time, Time::now());
        expect_tracks(vec![
            "update_next_heartbeat_time.end",
            "slow_down_heartbeating.end",
            "on_peer_disconnected.already_disconnected",
            "handle_failed_response.end",
        ]);
    }

    #[test]
    fn test_resume_heartbeating() {
        let test = new();

        test.start();
        init_tracks();

        let mut state = test.state.borrow_mut();
        state.current_heartbeat_period = Duration::from_secs(1);

        test.resume_heartbeating(&mut state);
        expect_tracks(vec!["update_next_heartbeat_time.end", "resume_heartbeating.end"]);
    }

    #[test]
    fn test_slow_down_heartbeating() {
        let test = new();

        test.start();
        init_tracks();

        let mut state = test.state.borrow_mut();
        let period = Duration::from_millis(500);
        state.current_heartbeat_period = period;

        test.slow_down_heartbeating(&mut state);

        assert_eq!(state.current_heartbeat_period, period + test.send_backoff_period);
        expect_tracks(vec!["update_next_heartbeat_time.end", "slow_down_heartbeating.end"]);
    }

    #[test]
    fn test_update_next_heartbeat_time() {
        let test = new();

        test.start();
        init_tracks();
        Time::advance(Duration::from_secs(10));

        let mut state = test.state.borrow_mut();

        test.update_next_heartbeat_time(&mut state);

        assert_eq!(state.next_heartbeat_time, Time::now() + state.current_heartbeat_period);
        expect_tracks(vec!["update_next_heartbeat_time.end"]);
    }

    #[test]
    fn test_on_peer_connected() {
        let test = new();

        test.start();
        init_tracks();

        let mut state = test.state.borrow_mut();
        state.in_progress = true;

        test.on_peer_connected(&mut state);

        assert_eq!(state.in_progress, false);
        assert_eq!(state.peer_timeout_timer.is_started(), true);
        expect_tracks(vec!["on_peer_connected.end"]);
    }

    #[test]
    fn test_on_peer_disconnected() {
        let mut setup = Setup::new();
        setup
            .message_sender_factory
            .create_sender(Result::Ok(MessageSenderMockBuilder::new().build()));
        let test = new_with_setup(setup);

        test.start();

        let mut state = test.state.borrow_mut();
        assert_eq!(test.ensure_message_sender(&mut state), true);

        state.in_progress = true;
        init_tracks();

        test.on_peer_disconnected(&mut state);

        assert_eq!(state.message_sender.is_none(), true);
        assert_eq!(state.in_progress, false);
        assert_eq!(state.peer_timeout_timer.is_started(), false);
        expect_tracks(vec!["on_peer_disconnected.end"]);
    }

    #[test]
    fn test_handle_peer_timeout() {
        let test = new();

        test.start();
        init_tracks();

        let mut state = test.state.borrow_mut();

        test.handle_peer_timeout(&mut state);

        expect_tracks(vec![
            "on_peer_disconnected.already_disconnected",
            "handle_peer_timeout.end",
        ]);
    }

    #[test]
    fn test_ensure_message_sender() {
        let mut setup = Setup::new();
        setup
            .message_sender_factory
            .create_sender(Result::Ok(MessageSenderMockBuilder::new().build()));
        let test = new_with_setup(setup);

        test.start();
        init_tracks();

        let mut state = test.state.borrow_mut();
        assert_eq!(test.ensure_message_sender(&mut state), true);

        assert_eq!(state.message_sender.is_some(), true);
        expect_tracks(vec!["ensure_message_sender.end", "on_peer_connected.end"]);
    }

    #[test]
    fn test_ensure_message_sender_failed() {
        let mut setup = Setup::new();
        setup
            .message_sender_factory
            .create_sender(Result::Err(Box::new(ErrorKind::TimeoutOccurred)));
        let test = new_with_setup(setup);

        test.start();
        init_tracks();

        let mut state = test.state.borrow_mut();
        assert_eq!(test.ensure_message_sender(&mut state), false);

        assert_eq!(state.message_sender.is_none(), true);
        expect_tracks(vec![
            "ensure_message_sender.end",
            "on_peer_disconnected.already_disconnected",
        ]);
    }

    fn new() -> ServerPeer {
        new_with_setup(Setup::new())
    }

    fn new_with_setup(setup: Setup) -> ServerPeer {
        let mut test_state = setup.build();
        test_state.server_state.peers.remove(0)
    }
}
