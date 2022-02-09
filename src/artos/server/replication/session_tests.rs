#[cfg(test)]
mod tests {
    use super::*;
    use crate::artos::server::test::Setup;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_session_receive() {
        let mut session = new_session();
        let message_id = 10;
        session.last_received_message_id = message_id;
        session.out_of_order_received = true;

        assert_eq!(session.receive(message_id + 1), true);
        assert_eq!(session.last_received_message_id, message_id + 1);
        assert_eq!(session.out_of_order_received, false);
    }

    #[test]
    fn test_session_receive_out_of_order() {
        let mut session = new_session();
        let message_id = 10;
        session.last_received_message_id = message_id;

        assert_eq!(session.receive(message_id + 2), false);

        assert_eq!(session.last_received_message_id, message_id);
        assert_eq!(session.out_of_order_received, true);
    }

    #[test]
    fn test_session_receive_ignored() {
        let mut session = new_session();
        let message_id = 10;
        session.last_received_message_id = message_id;

        assert_eq!(session.receive(message_id - 1), false);

        assert_eq!(session.last_received_message_id, message_id);
        assert_eq!(session.out_of_order_received, false);
    }

    #[test]
    fn test_session_commit() {
        let mut session = new_session();
        let message_id = 10;
        session.last_received_message_id = message_id + 11;
        session.last_committed_message_id = message_id;

        session.commit(message_id + 10);

        assert_eq!(session.last_committed_message_id, message_id + 10);
        assert_eq!(session.last_received_message_id, message_id + 11);
        assert_eq!(session.committed, true);
    }

    #[test]
    fn test_session_commit_ignored() {
        let mut session = new_session();
        let message_id = 10;
        session.last_committed_message_id = message_id;

        session.commit(message_id - 1);

        assert_eq!(session.last_committed_message_id, message_id);
        assert_eq!(session.committed, true);
    }

    #[test]
    fn test_session_commit_fix_received() {
        let mut session = new_session();
        let message_id = 10;
        session.last_received_message_id = message_id;
        session.last_committed_message_id = message_id;
        session.out_of_order_received = true;

        session.commit(message_id + 10);

        assert_eq!(session.last_committed_message_id, message_id + 10);
        assert_eq!(session.last_received_message_id, message_id + 10);
        assert_eq!(session.out_of_order_received, false);
        assert_eq!(session.committed, true);
    }

    #[test]
    fn test_session_manager_ensure_session() {
        let mut session_manager = new_session_manager();

        let client_id = Uuid::new_v4();
        let session = session_manager.ensure_session(client_id);

        assert_eq!(session.last_access_time, Time::now());
        let session_addr = Setup::address(session);
        assert_eq!(session_manager.sessions.len(), 1);

        Time::advance(Duration::from_secs(1));

        let session2 = session_manager.ensure_session(client_id);
        assert_eq!(session2.last_access_time, Time::now());
        let session_addr2 = Setup::address(session2);
        assert_eq!(session_manager.sessions.len(), 1);
        assert_eq!(session_addr, session_addr2);
    }

    #[test]
    fn test_session_manager_check_timed_out_sessions() {
        let mut session_manager = new_session_manager();

        let client_id1 = Uuid::new_v4();
        session_manager.ensure_session(client_id1);

        Time::advance(Duration::from_secs(50));

        let client_id2 = Uuid::new_v4();
        session_manager.ensure_session(client_id2);

        assert_eq!(session_manager.check_timed_out_sessions(Time::now()).is_none(), true);

        Time::advance(Duration::from_secs(51));

        let result = session_manager.check_timed_out_sessions(Time::now()).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].client_id, client_id1);
    }

    #[test]
    fn test_session_manager_clear() {
        let mut session_manager = new_session_manager();

        let client_id = Uuid::new_v4();
        session_manager.ensure_session(client_id);
        session_manager.clear();
        assert_eq!(session_manager.sessions.is_empty(), true);
    }

    fn new_session() -> ClientSession {
        Setup::init_time();

        ClientSession::new(Uuid::new_v4())
    }

    fn new_session_manager() -> ClientSessionManager {
        Setup::init_time();

        ClientSessionManager::new(Duration::from_secs(100))
    }
}
