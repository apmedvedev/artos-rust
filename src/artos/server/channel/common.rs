use uuid::Uuid;

use crate::artos::common::{GroupConfiguration, MessageSenderFactory, ServerConfiguration};
use crate::artos::server::api::{
    LogStore, MessageListenerFactory, PersistentServerState, ServerChannelConfiguration,
    ServerChannelFactoryConfiguration, StateMachine,
};
use crate::artos::server::channel::{Event, ServerPeer};
use crate::compartment::Sender;

//////////////////////////////////////////////////////////
// ServerRole
//////////////////////////////////////////////////////////

#[derive(PartialEq, Debug, Clone, Copy)]
pub enum ServerRole {
    Follower,
    Candidate,
    Leader,
}

//////////////////////////////////////////////////////////
// ServerState
//////////////////////////////////////////////////////////

pub struct ServerState {
    pub configuration: GroupConfiguration,
    pub role: ServerRole,
    pub persistent: PersistentServerState,
    pub leader_id: Option<Uuid>,
    pub peers: Vec<ServerPeer>,
    pub quick_commit_index: u64,
    pub configuration_changing: bool,
    pub catching_up: bool,
}

impl ServerState {
    pub fn new(configuration: GroupConfiguration) -> ServerState {
        ServerState {
            configuration,
            role: ServerRole::Follower,
            persistent: Default::default(),
            leader_id: None,
            peers: vec![],
            quick_commit_index: 0,
            configuration_changing: false,
            catching_up: false,
        }
    }
}

//////////////////////////////////////////////////////////
// Context
//////////////////////////////////////////////////////////
pub struct Context {
    local_server: ServerConfiguration,
    group_id: Uuid,
    message_listener_factory: Box<dyn MessageListenerFactory>,
    message_sender_factory: Box<dyn MessageSenderFactory>,
    log_store: Box<dyn LogStore>,
    state_machine: Box<dyn StateMachine>,
    server_channel_factory_configuration: ServerChannelFactoryConfiguration,
    server_channel_configuration: ServerChannelConfiguration,
    sender: Sender<Event>,
}

impl Context {
    pub fn new(
        local_server: ServerConfiguration,
        group_id: Uuid,
        message_listener_factory: Box<dyn MessageListenerFactory>,
        message_sender_factory: Box<dyn MessageSenderFactory>,
        log_store: Box<dyn LogStore>,
        state_machine: Box<dyn StateMachine>,
        server_channel_factory_configuration: ServerChannelFactoryConfiguration,
        server_channel_configuration: ServerChannelConfiguration,
        sender: Sender<Event>,
    ) -> Context {
        Context {
            local_server,
            group_id,
            message_listener_factory,
            message_sender_factory,
            log_store,
            state_machine,
            server_channel_factory_configuration,
            server_channel_configuration,
            sender,
        }
    }

    pub fn local_server(&self) -> &ServerConfiguration {
        &self.local_server
    }

    pub fn group_id(&self) -> Uuid {
        self.group_id
    }

    pub fn message_listener_factory(&self) -> &Box<dyn MessageListenerFactory> {
        &self.message_listener_factory
    }

    pub fn message_sender_factory(&self) -> &Box<dyn MessageSenderFactory> {
        &self.message_sender_factory
    }

    pub fn log_store(&self) -> &Box<dyn LogStore> {
        &self.log_store
    }

    pub fn state_machine(&self) -> &Box<dyn StateMachine> {
        &self.state_machine
    }

    pub fn server_channel_factory_configuration(&self) -> &ServerChannelFactoryConfiguration {
        &self.server_channel_factory_configuration
    }

    pub fn server_channel_configuration(&self) -> &ServerChannelConfiguration {
        &self.server_channel_configuration
    }

    pub fn sender(&self) -> &Sender<Event> {
        &self.sender
    }
}
