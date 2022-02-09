use crate::artos::common::utils::VecExt;
use crate::artos::common::ResponseHandler;
use crate::artos::message::client::QueryRequestItem;
use crate::common::utils::Time;
use crate::compartment::Timer;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

include!("session_tests.rs");

//////////////////////////////////////////////////////////
// ClientSession
//////////////////////////////////////////////////////////

pub type QueryInfo = (QueryRequestItem, Arc<dyn ResponseHandler>);

#[derive(Clone)]
pub struct ClientSession {
    client_id: Uuid,
    last_received_message_id: u64,
    last_committed_message_id: u64,
    out_of_order_received: bool,
    last_access_time: Instant,
    query_queue: VecDeque<QueryInfo>,
    committed: bool,
    subscriptions: Vec<Uuid>,
    last_update_subscription_time: Instant,
}

impl ClientSession {
    pub fn new(client_id: Uuid) -> ClientSession {
        ClientSession {
            client_id,
            last_received_message_id: 0,
            last_committed_message_id: 0,
            out_of_order_received: false,
            last_access_time: Time::now(),
            query_queue: VecDeque::new(),
            committed: false,
            subscriptions: vec![],
            last_update_subscription_time: Time::now(),
        }
    }

    pub fn client_id(&self) -> Uuid {
        self.client_id
    }

    pub fn last_received_message_id(&self) -> u64 {
        self.last_received_message_id
    }

    pub fn set_last_received_message_id(&mut self, value: u64) {
        self.last_received_message_id = value;
    }

    pub fn last_committed_message_id(&self) -> u64 {
        self.last_committed_message_id
    }

    pub fn set_last_committed_message_id(&mut self, value: u64) {
        self.last_committed_message_id = value;
    }

    pub fn is_out_of_order_received(&self) -> bool {
        self.out_of_order_received
    }

    pub fn set_out_of_order_received(&mut self, value: bool) {
        self.out_of_order_received = value;
    }

    pub fn last_access_time(&self) -> Instant {
        self.last_access_time
    }

    pub fn set_last_access_time(&mut self, value: Instant) {
        self.last_access_time = value;
    }

    pub fn query_queue(&mut self) -> &mut VecDeque<QueryInfo> {
        &mut self.query_queue
    }

    pub fn add_query(&mut self, query: QueryInfo) {
        self.query_queue.push_back(query)
    }

    pub fn is_committed(&self) -> bool {
        self.committed
    }

    pub fn set_committed(&mut self, value: bool) {
        self.committed = value;
    }

    pub fn subscriptions(&self) -> &Vec<Uuid> {
        &self.subscriptions
    }

    pub fn set_subscriptions(&mut self, value: Vec<Uuid>) {
        self.subscriptions = value;
        self.last_update_subscription_time = Time::now();
    }

    pub fn last_update_subscription_time(&self) -> Instant {
        self.last_update_subscription_time
    }

    pub fn receive(&mut self, message_id: u64) -> bool {
        if message_id == self.last_received_message_id + 1 {
            self.last_received_message_id = message_id;
            self.out_of_order_received = false;
            return true;
        } else if message_id > self.last_received_message_id + 1 {
            self.out_of_order_received = true;
        }

        false
    }

    pub fn commit(&mut self, message_id: u64) {
        if message_id > self.last_committed_message_id {
            self.last_committed_message_id = message_id;
        }

        if self.last_received_message_id < self.last_committed_message_id {
            self.last_received_message_id = self.last_committed_message_id;
            self.out_of_order_received = false;
        }

        self.committed = true;
    }
}

//////////////////////////////////////////////////////////
// ClientSessionManager
//////////////////////////////////////////////////////////

pub struct ClientSessionManager {
    client_session_timeout: Duration,
    sessions: Vec<ClientSession>,
    session_timeout_timer: Timer,
}

impl ClientSessionManager {
    pub fn new(client_session_timeout: Duration) -> ClientSessionManager {
        ClientSessionManager {
            client_session_timeout,
            sessions: vec![],
            session_timeout_timer: Timer::new(true, client_session_timeout / 2),
        }
    }

    pub fn sessions(&self) -> &Vec<ClientSession> {
        &self.sessions
    }

    pub fn sessions_mut(&mut self) -> &mut Vec<ClientSession> {
        &mut self.sessions
    }

    pub fn ensure_session(&mut self, client_id: Uuid) -> &mut ClientSession {
        let index = self.sessions.iter().position(|v| v.client_id() == client_id);

        if let Some(index) = index {
            let session = &mut self.sessions[index];
            session.set_last_access_time(Time::now());
            session
        } else {
            self.sessions.push(ClientSession::new(client_id));
            self.sessions.last_mut().unwrap()
        }
    }

    pub fn has_session(&self, client_id: Uuid) -> bool {
        self.sessions.exists(|v| v.client_id() == client_id)
    }

    pub fn check_timed_out_sessions(&mut self, current_time: Instant) -> Option<Vec<ClientSession>> {
        if self.session_timeout_timer.is_fired(current_time) {
            let client_session_timeout = self.client_session_timeout;

            let mut sessions: Vec<ClientSession> = vec![];
            self.sessions.retain(|session| {
                if current_time < session.last_access_time + client_session_timeout {
                    true
                } else {
                    sessions.push(session.clone());
                    false
                }
            });

            if !sessions.is_empty() {
                return Some(sessions);
            }
        }

        None
    }

    pub fn clear(&mut self) -> Vec<ClientSession> {
        let mut sessions = vec![];
        std::mem::swap(&mut sessions, &mut self.sessions);
        sessions
    }
}
