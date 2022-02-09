pub mod client;
pub mod server;

use crate::artos::message::client::{
    ConfigurationRequest, PublishRequest, PublishResponse, QueryRequest, QueryResponse, RestartSessionRequest,
    SubscriptionRequest, SubscriptionResponse,
};
use crate::artos::message::server::{
    AppendEntriesRequest, AppendEntriesResponse, JoinGroupRequest, JoinGroupResponse, LeaveGroupRequest,
    LeaveGroupResponse, VoteRequest, VoteResponse,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

//////////////////////////////////////////////////////////
// Message
//////////////////////////////////////////////////////////

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum Request {
    Publish(PublishRequest),
    Query(QueryRequest),
    Subscribe(SubscriptionRequest),
    RestartSession(RestartSessionRequest),
    Configuration(ConfigurationRequest),
    Vote(VoteRequest),
    AppendEntries(AppendEntriesRequest),
    JoinGroup(JoinGroupRequest),
    LeaveGroup(LeaveGroupRequest),
}

impl Request {
    pub fn id(&self) -> Uuid {
        match self {
            Self::Publish(request) => request.client_id(),
            Self::Query(request) => request.client_id(),
            Self::Subscribe(request) => request.client_id(),
            Self::RestartSession(request) => request.client_id(),
            Self::Configuration(request) => request.client_id(),
            Self::Vote(request) => request.server_id(),
            Self::AppendEntries(request) => request.server_id(),
            Self::JoinGroup(request) => request.server_id(),
            Self::LeaveGroup(request) => request.server_id(),
        }
    }

    pub fn group_id(&self) -> Uuid {
        match self {
            Self::Publish(request) => request.group_id(),
            Self::Query(request) => request.group_id(),
            Self::Subscribe(request) => request.group_id(),
            Self::RestartSession(request) => request.group_id(),
            Self::Configuration(request) => request.group_id(),
            Self::Vote(request) => request.group_id(),
            Self::AppendEntries(request) => request.group_id(),
            Self::JoinGroup(request) => request.group_id(),
            Self::LeaveGroup(request) => request.group_id(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Response {
    Publish(PublishResponse),
    Query(QueryResponse),
    Subscribe(SubscriptionResponse),
    Vote(VoteResponse),
    AppendEntries(AppendEntriesResponse),
    JoinGroup(JoinGroupResponse),
    LeaveGroup(LeaveGroupResponse),
}

impl Response {
    pub fn server_id(&self) -> Uuid {
        match self {
            Self::Publish(response) => response.server_id(),
            Self::Query(response) => response.server_id(),
            Self::Subscribe(response) => response.server_id(),
            Self::Vote(response) => response.server_id(),
            Self::AppendEntries(response) => response.server_id(),
            Self::JoinGroup(response) => response.server_id(),
            Self::LeaveGroup(response) => response.server_id(),
        }
    }
}
