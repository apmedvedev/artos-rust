#[cfg(test)]
mod tests {
    use super::*;
    use crate::artos::common::ErrorKind;
    use crate::artos::server::test::{MessageSenderMockBuilder, Setup};
    use crate::common::utils::expect_tracks;
    use crate::common::utils::init_tracks;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_stop() {
        let (test, _) = new();

        test.stop();

        expect_tracks(vec!["on_leader_disconnected.end", "stop.end"]);
    }

    #[test]
    fn test_send() {
        let mut setup = Setup::new();

        let request = setup.join_group_request.build();
        setup
            .message_sender_factory
            .create_sender(Result::Ok(MessageSenderMockBuilder::new().send().build()));

        let (test, mut server_state) = new_with_setup(setup);

        test.send(Request::JoinGroup(request), &mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.message_sender.is_some(), true);
        expect_tracks(vec![
            "select_leader_configuration.random_leader",
            "ensure_leader_configuration.end",
            "ensure_message_sender.end",
            "send.send",
            "send.end",
        ]);
    }

    #[test]
    fn test_handle_succeeded_response_accepted() {
        let (test, _) = new();

        assert_eq!(
            test.handle_succeeded_response(
                test.context.local_server().clone(),
                true,
                Some(test.context.local_server().id())
            ),
            true
        );

        expect_tracks(vec!["handle_succeeded_response.end"]);
    }

    #[test]
    fn test_handle_succeeded_response_rejected_by_new_leader() {
        let mut setup = Setup::new();

        setup
            .message_sender_factory
            .create_sender(Result::Ok(MessageSenderMockBuilder::new().build()));

        let (test, server_state) = new_with_setup(setup);

        let leader_id = Uuid::new_v4();
        {
            let mut state = test.state.borrow_mut();
            assert_eq!(test.ensure_message_sender(&mut state, &server_state), true);
        }

        assert_eq!(
            test.handle_succeeded_response(test.context.local_server().clone(), false, Some(leader_id)),
            false
        );

        let state = test.state.borrow();
        assert_eq!(state.leader_id, Some(leader_id));
        assert_eq!(state.message_sender.is_none(), true);
        expect_tracks(vec![
            "select_leader_configuration.random_leader",
            "ensure_leader_configuration.end",
            "ensure_message_sender.end",
            "handle_succeeded_response.rejected",
            "on_leader_disconnected.end",
        ]);
    }

    #[test]
    fn test_handle_succeeded_response_rejected_by_none_leader() {
        let mut setup = Setup::new();

        setup
            .message_sender_factory
            .create_sender(Result::Ok(MessageSenderMockBuilder::new().build()));

        let (test, server_state) = new_with_setup(setup);

        {
            let mut state = test.state.borrow_mut();
            assert_eq!(test.ensure_message_sender(&mut state, &server_state), true);
        }

        assert_eq!(
            test.handle_succeeded_response(test.context.local_server().clone(), false, None),
            false
        );

        let state = test.state.borrow();
        assert_eq!(state.leader_id, None);
        assert_eq!(state.message_sender.is_none(), true);
        expect_tracks(vec![
            "select_leader_configuration.random_leader",
            "ensure_leader_configuration.end",
            "ensure_message_sender.end",
            "handle_succeeded_response.rejected",
            "on_leader_disconnected.end",
        ]);
    }

    #[test]
    fn test_handle_failed_response() {
        let (test, _) = new();

        test.handle_failed_response(
            test.context.local_server().clone(),
            Box::new(ErrorKind::TimeoutOccurred),
        );

        expect_tracks(vec!["on_leader_disconnected.end", "handle_failed_response.end"]);
    }

    #[test]
    fn test_on_leader_disconnected() {
        let mut setup = Setup::new();

        setup
            .message_sender_factory
            .create_sender(Result::Ok(MessageSenderMockBuilder::new().build()));

        let (test, server_state) = new_with_setup(setup);

        let mut state = test.state.borrow_mut();
        assert_eq!(test.ensure_message_sender(&mut state, &server_state), true);
        init_tracks();

        test.on_leader_disconnected(&mut state);

        assert_eq!(state.leader_id, None);
        assert_eq!(state.leader_endpoint, None);
        assert_eq!(state.message_sender.is_none(), true);
        expect_tracks(vec!["on_leader_disconnected.end"]);
    }

    #[test]
    fn test_ensure_message_sender() {
        let mut setup = Setup::new();

        setup
            .message_sender_factory
            .create_sender(Result::Ok(MessageSenderMockBuilder::new().build()));

        let (test, server_state) = new_with_setup(setup);

        let mut state = test.state.borrow_mut();
        assert_eq!(test.ensure_message_sender(&mut state, &server_state), true);
        assert_eq!(test.ensure_message_sender(&mut state, &server_state), true);

        assert_eq!(state.leader_id.is_some(), true);
        assert_eq!(state.leader_endpoint.is_some(), true);
        assert_eq!(state.message_sender.is_some(), true);
        expect_tracks(vec![
            "select_leader_configuration.random_leader",
            "ensure_leader_configuration.end",
            "ensure_message_sender.end",
        ]);
    }

    #[test]
    fn test_ensure_message_sender_error() {
        let mut setup = Setup::new();

        setup
            .message_sender_factory
            .create_sender(Result::Err(Box::new(ErrorKind::TimeoutOccurred)));

        let (test, server_state) = new_with_setup(setup);

        let mut state = test.state.borrow_mut();
        state.leader_endpoint = Some(String::from("test"));
        state.leader_id = Some(Uuid::new_v4());

        assert_eq!(test.ensure_message_sender(&mut state, &server_state), false);

        assert_eq!(state.leader_id.is_none(), true);
        assert_eq!(state.leader_endpoint.is_none(), true);
        assert_eq!(state.message_sender.is_none(), true);
        expect_tracks(vec![
            "select_leader_configuration.random_leader",
            "ensure_leader_configuration.end",
            "ensure_message_sender.error",
            "on_leader_disconnected.end",
        ]);
    }

    #[test]
    fn test_ensure_leader_configuration() {
        let (test, server_state) = new();

        let mut state = test.state.borrow_mut();

        let (leader_id, leader_endpoint) = test.ensure_leader_configuration(&mut state, &server_state);

        assert_eq!(state.leader_id, Some(leader_id));
        assert_eq!(state.leader_endpoint, Some(leader_endpoint));
        expect_tracks(vec![
            "select_leader_configuration.random_leader",
            "ensure_leader_configuration.end",
        ]);
    }

    #[test]
    fn test_select_leader_configuration_random() {
        let (test, server_state) = new();

        test.select_leader_configuration(Some(Uuid::new_v4()), &server_state);

        expect_tracks(vec!["select_leader_configuration.random_leader"]);
    }

    #[test]
    fn test_select_leader_configuration_local() {
        let (test, server_state) = new();

        test.select_leader_configuration(Some(test.context.local_server().id()), &server_state);

        expect_tracks(vec!["select_leader_configuration.random_leader"]);
    }

    #[test]
    fn test_select_leader_configuration() {
        let (test, server_state) = new();

        test.select_leader_configuration(Some(server_state.configuration.servers()[1].id()), &server_state);

        expect_tracks(vec!["select_leader_configuration.leader_found"]);
    }

    fn new() -> (FollowerMessageSenderImpl, ServerState) {
        new_with_setup(Setup::new())
    }

    fn new_with_setup(setup: Setup) -> (FollowerMessageSenderImpl, ServerState) {
        let (sender, _) = setup.sender.build();
        let test_state = setup.build();
        (
            FollowerMessageSenderImpl::new(test_state.context, FollowerType::Join, sender),
            test_state.server_state,
        )
    }
}
