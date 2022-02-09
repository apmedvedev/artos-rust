use crate::artos::common::api::GroupConfiguration;
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use std::sync::Arc;
use uuid::Uuid;

//////////////////////////////////////////////////////////
// LogEntry
//////////////////////////////////////////////////////////

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LogEntry {
    message_id: u64,
    value: Arc<Vec<u8>>,
}

impl LogEntry {
    pub fn new(message_id: u64, value: Vec<u8>) -> LogEntry {
        LogEntry {
            message_id,
            value: Arc::new(value),
        }
    }

    pub fn message_id(&self) -> u64 {
        self.message_id
    }

    pub fn value(&self) -> &Arc<Vec<u8>> {
        &self.value
    }
}

//////////////////////////////////////////////////////////
// PublishRequest
//////////////////////////////////////////////////////////

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct PublishRequest {
    client_id: Uuid,
    group_id: Uuid,
    entries: Vec<LogEntry>,
}

impl PublishRequest {
    pub fn new(client_id: Uuid, group_id: Uuid, entries: Vec<LogEntry>) -> PublishRequest {
        PublishRequest {
            client_id,
            group_id,
            entries,
        }
    }

    pub fn client_id(&self) -> Uuid {
        self.client_id
    }

    pub fn group_id(&self) -> Uuid {
        self.group_id
    }

    pub fn entries(&self) -> &Vec<LogEntry> {
        &self.entries
    }
}

//////////////////////////////////////////////////////////
// PublishResponse
//////////////////////////////////////////////////////////

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PublishResponse {
    server_id: Uuid,
    accepted: bool,
    leader_id: Option<Uuid>,
    configuration: Option<GroupConfiguration>,
    out_of_order_received: bool,
    last_received_message_id: u64,
    last_committed_message_id: u64,
}

impl PublishResponse {
    pub fn new(
        server_id: Uuid,
        accepted: bool,
        leader_id: Option<Uuid>,
        configuration: Option<GroupConfiguration>,
        out_of_order_received: bool,
        last_received_message_id: u64,
        last_committed_message_id: u64,
    ) -> PublishResponse {
        PublishResponse {
            server_id,
            accepted,
            leader_id,
            configuration,
            out_of_order_received,
            last_received_message_id,
            last_committed_message_id,
        }
    }

    pub fn server_id(&self) -> Uuid {
        self.server_id
    }

    pub fn is_accepted(&self) -> bool {
        self.accepted
    }

    pub fn leader_id(&self) -> Option<Uuid> {
        self.leader_id
    }

    pub fn configuration(&self) -> &Option<GroupConfiguration> {
        &self.configuration
    }

    pub fn is_out_of_order_received(&self) -> bool {
        self.out_of_order_received
    }

    pub fn last_received_message_id(&self) -> u64 {
        self.last_received_message_id
    }

    pub fn last_committed_message_id(&self) -> u64 {
        self.last_committed_message_id
    }
}

//////////////////////////////////////////////////////////
// QueryRequest
//////////////////////////////////////////////////////////

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct QueryRequest {
    client_id: Uuid,
    group_id: Uuid,
    queries: Vec<QueryRequestItem>,
}

impl QueryRequest {
    pub fn new(client_id: Uuid, group_id: Uuid, queries: Vec<QueryRequestItem>) -> QueryRequest {
        QueryRequest {
            client_id,
            group_id,
            queries,
        }
    }

    pub fn client_id(&self) -> Uuid {
        self.client_id
    }
    pub fn group_id(&self) -> Uuid {
        self.group_id
    }
    pub fn queries(&self) -> &Vec<QueryRequestItem> {
        &self.queries
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct QueryRequestItem {
    query_id: u64,
    last_published_message_id: u64,
    request: Rc<Vec<u8>>,
}

impl QueryRequestItem {
    pub fn new(query_id: u64, last_published_message_id: u64, request: Vec<u8>) -> QueryRequestItem {
        QueryRequestItem {
            query_id,
            last_published_message_id,
            request: Rc::new(request),
        }
    }

    pub fn query_id(&self) -> u64 {
        self.query_id
    }

    pub fn last_published_message_id(&self) -> u64 {
        self.last_published_message_id
    }

    pub fn request(&self) -> &Vec<u8> {
        &self.request
    }
}

//////////////////////////////////////////////////////////
// QueryResponse
//////////////////////////////////////////////////////////

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QueryResponse {
    server_id: Uuid,
    accepted: bool,
    results: Vec<QueryResponseItem>,
}

impl QueryResponse {
    pub fn new(server_id: Uuid, accepted: bool, results: Vec<QueryResponseItem>) -> QueryResponse {
        QueryResponse {
            server_id,
            accepted,
            results,
        }
    }
    pub fn server_id(&self) -> Uuid {
        self.server_id
    }

    pub fn is_accepted(&self) -> bool {
        self.accepted
    }

    pub fn results(&self) -> &Vec<QueryResponseItem> {
        &self.results
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QueryResponseItem {
    query_id: u64,
    value: Vec<u8>,
}

impl QueryResponseItem {
    pub fn new(query_id: u64, value: Vec<u8>) -> QueryResponseItem {
        QueryResponseItem { query_id, value }
    }

    pub fn query_id(&self) -> u64 {
        self.query_id
    }

    pub fn value(&self) -> &Vec<u8> {
        &self.value
    }
}

//////////////////////////////////////////////////////////
// SubscriptionRequest
//////////////////////////////////////////////////////////

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SubscriptionRequest {
    client_id: Uuid,
    group_id: Uuid,
    subscriptions: Vec<SubscriptionRequestItem>,
}

impl SubscriptionRequest {
    pub fn new(client_id: Uuid, group_id: Uuid, subscriptions: Vec<SubscriptionRequestItem>) -> SubscriptionRequest {
        SubscriptionRequest {
            client_id,
            group_id,
            subscriptions,
        }
    }

    pub fn client_id(&self) -> Uuid {
        self.client_id
    }

    pub fn group_id(&self) -> Uuid {
        self.group_id
    }

    pub fn subscriptions(&self) -> &Vec<SubscriptionRequestItem> {
        &self.subscriptions
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SubscriptionRequestItem {
    subscription_id: Uuid,
    request: Rc<Vec<u8>>,
}

impl SubscriptionRequestItem {
    pub fn new(subscription_id: Uuid, request: Vec<u8>) -> SubscriptionRequestItem {
        SubscriptionRequestItem {
            subscription_id,
            request: Rc::new(request),
        }
    }

    pub fn subscription_id(&self) -> Uuid {
        self.subscription_id
    }

    pub fn request(&self) -> &Vec<u8> {
        &self.request
    }
}

//////////////////////////////////////////////////////////
// SubscriptionResponse
//////////////////////////////////////////////////////////

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SubscriptionResponse {
    server_id: Uuid,
    accepted: bool,
    results: Vec<SubscriptionResponseItem>,
}

impl SubscriptionResponse {
    pub fn new(server_id: Uuid, accepted: bool, results: Vec<SubscriptionResponseItem>) -> SubscriptionResponse {
        SubscriptionResponse {
            server_id,
            accepted,
            results,
        }
    }
    pub fn server_id(&self) -> Uuid {
        self.server_id
    }

    pub fn is_accepted(&self) -> bool {
        self.accepted
    }

    pub fn results(&self) -> &Vec<SubscriptionResponseItem> {
        &self.results
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SubscriptionResponseItem {
    subscription_id: Uuid,
    value: Vec<u8>,
}

impl SubscriptionResponseItem {
    pub fn new(subscription_id: Uuid, value: Vec<u8>) -> SubscriptionResponseItem {
        SubscriptionResponseItem { subscription_id, value }
    }

    pub fn subscription_id(&self) -> Uuid {
        self.subscription_id
    }

    pub fn value(&self) -> &Vec<u8> {
        &self.value
    }
}

//////////////////////////////////////////////////////////
// RestartSessionRequest
//////////////////////////////////////////////////////////

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct RestartSessionRequest {
    client_id: Uuid,
    group_id: Uuid,
    last_sent_message_id: u64,
}

impl RestartSessionRequest {
    pub fn new(client_id: Uuid, group_id: Uuid, last_sent_message_id: u64) -> RestartSessionRequest {
        RestartSessionRequest {
            client_id,
            group_id,
            last_sent_message_id,
        }
    }

    pub fn client_id(&self) -> Uuid {
        self.client_id
    }

    pub fn group_id(&self) -> Uuid {
        self.group_id
    }

    pub fn last_sent_message_id(&self) -> u64 {
        self.last_sent_message_id
    }
}

//////////////////////////////////////////////////////////
// ConfigurationRequest
//////////////////////////////////////////////////////////

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ConfigurationRequest {
    client_id: Uuid,
    group_id: Uuid,
}

impl ConfigurationRequest {
    pub fn new(client_id: Uuid, group_id: Uuid) -> ConfigurationRequest {
        ConfigurationRequest { client_id, group_id }
    }

    pub fn client_id(&self) -> Uuid {
        self.client_id
    }

    pub fn group_id(&self) -> Uuid {
        self.group_id
    }
}
