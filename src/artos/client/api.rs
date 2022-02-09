use crate::artos::common::{GroupConfiguration, MessageSenderConfiguration};
use std::fmt::Debug;
use std::time::Duration;
use uuid::Uuid;

//////////////////////////////////////////////////////////
// ClientChannelFactoryConfiguration
//////////////////////////////////////////////////////////

#[derive(PartialEq, Debug, Clone)]
pub struct ClientChannelFactoryConfiguration {
    heartbeat_period: Duration,
    update_configuration_period: Duration,
    lock_queue_capacity: usize,
    unlock_queue_capacity: usize,
    timer_period: Duration,
    server_timeout: Duration,
    max_value_size: usize,
    max_batch_size: usize,
    resend_query_period: Duration,
    resubscription_period: Duration,
}

impl ClientChannelFactoryConfiguration {
    fn new() -> ClientChannelFactoryConfiguration {
        Default::default()
    }

    pub fn heartbeat_period(&self) -> Duration {
        self.heartbeat_period
    }

    pub fn set_heartbeat_period(&mut self, value: Duration) -> &mut ClientChannelFactoryConfiguration {
        self.heartbeat_period = value;
        self
    }

    pub fn update_configuration_period(&self) -> Duration {
        self.update_configuration_period
    }

    pub fn set_update_configuration_period(&mut self, value: Duration) -> &mut ClientChannelFactoryConfiguration {
        self.update_configuration_period = value;
        self
    }

    pub fn lock_queue_capacity(&self) -> usize {
        self.lock_queue_capacity
    }

    pub fn set_lock_queue_capacity(&mut self, value: usize) -> &mut ClientChannelFactoryConfiguration {
        self.lock_queue_capacity = value;
        self
    }

    pub fn unlock_queue_capacity(&self) -> usize {
        self.unlock_queue_capacity
    }

    pub fn set_unlock_queue_capacity(&mut self, value: usize) -> &mut ClientChannelFactoryConfiguration {
        self.unlock_queue_capacity = value;
        self
    }

    pub fn timer_period(&self) -> Duration {
        self.timer_period
    }

    pub fn set_timer_period(&mut self, value: Duration) -> &mut ClientChannelFactoryConfiguration {
        self.timer_period = value;
        self
    }

    pub fn server_timeout(&self) -> Duration {
        self.server_timeout
    }

    pub fn set_server_timeout(&mut self, value: Duration) -> &mut ClientChannelFactoryConfiguration {
        self.server_timeout = value;
        self
    }

    pub fn max_value_size(&self) -> usize {
        self.max_value_size
    }

    pub fn set_max_value_size(&mut self, value: usize) -> &mut ClientChannelFactoryConfiguration {
        self.max_value_size = value;
        self
    }

    pub fn max_batch_size(&self) -> usize {
        self.max_batch_size
    }

    pub fn set_max_batch_size(&mut self, value: usize) -> &mut ClientChannelFactoryConfiguration {
        self.max_batch_size = value;
        self
    }

    pub fn resend_query_period(&self) -> Duration {
        self.resend_query_period
    }

    pub fn set_resend_query_period(&mut self, value: Duration) -> &mut ClientChannelFactoryConfiguration {
        self.resend_query_period = value;
        self
    }

    pub fn resubscription_period(&self) -> Duration {
        self.resubscription_period
    }

    pub fn set_resubscription_period(&mut self, value: Duration) -> &mut ClientChannelFactoryConfiguration {
        self.resubscription_period = value;
        self
    }
}

impl Default for ClientChannelFactoryConfiguration {
    fn default() -> Self {
        ClientChannelFactoryConfiguration {
            heartbeat_period: Duration::from_millis(500),
            update_configuration_period: Duration::from_millis(2000),
            lock_queue_capacity: 200,
            unlock_queue_capacity: 140,
            timer_period: Duration::from_millis(50),
            server_timeout: Duration::from_millis(2000),
            max_value_size: 1000_000,
            max_batch_size: 10_000_000,
            resend_query_period: Duration::from_secs(600),
            resubscription_period: Duration::from_secs(60),
        }
    }
}

//////////////////////////////////////////////////////////
// ClientChannelConfiguration
//////////////////////////////////////////////////////////

pub struct ClientChannelConfigurationBuilder {
    endpoint: Option<String>,
    group: Option<GroupConfiguration>,
    message_sender: Option<Box<dyn MessageSenderConfiguration>>,
    client_store: Option<Box<dyn ClientStoreConfiguration>>,
}

impl ClientChannelConfigurationBuilder {
    pub fn new() -> ClientChannelConfigurationBuilder {
        ClientChannelConfigurationBuilder {
            endpoint: None,
            group: None,
            message_sender: None,
            client_store: None,
        }
    }

    pub fn set_endpoint(mut self, value: String) -> ClientChannelConfigurationBuilder {
        self.endpoint = Some(value);
        self
    }

    pub fn set_group(mut self, value: GroupConfiguration) -> ClientChannelConfigurationBuilder {
        self.group = Some(value);
        self
    }

    pub fn set_message_sender(
        mut self,
        value: Box<dyn MessageSenderConfiguration>,
    ) -> ClientChannelConfigurationBuilder {
        self.message_sender = Some(value);
        self
    }

    pub fn set_client_store(mut self, value: Box<dyn ClientStoreConfiguration>) -> ClientChannelConfigurationBuilder {
        self.client_store = Some(value);
        self
    }

    pub fn build(self) -> ClientChannelConfiguration {
        ClientChannelConfiguration {
            endpoint: self.endpoint.unwrap(),
            group: self.group.unwrap(),
            message_sender: self.message_sender.unwrap(),
            client_store: self.client_store.unwrap(),
        }
    }
}

pub struct ClientChannelConfiguration {
    endpoint: String,
    group: GroupConfiguration,
    message_sender: Box<dyn MessageSenderConfiguration>,
    client_store: Box<dyn ClientStoreConfiguration>,
}

impl ClientChannelConfiguration {
    pub fn new(
        endpoint: String,
        group: GroupConfiguration,
        message_sender: Box<dyn MessageSenderConfiguration>,
        client_store: Box<dyn ClientStoreConfiguration>,
    ) -> ClientChannelConfiguration {
        ClientChannelConfiguration {
            endpoint,
            group,
            message_sender,
            client_store,
        }
    }

    pub fn endpoint(&self) -> &String {
        &self.endpoint
    }

    pub fn group(&self) -> &GroupConfiguration {
        &self.group
    }

    pub fn message_sender(&self) -> &Box<dyn MessageSenderConfiguration> {
        &self.message_sender
    }

    pub fn client_store(&self) -> &Box<dyn ClientStoreConfiguration> {
        &self.client_store
    }
}

//////////////////////////////////////////////////////////
// Client SPI
//////////////////////////////////////////////////////////

pub trait ClientStoreConfiguration: Debug + Send + 'static {
    fn build(&self) -> Box<dyn ClientStore>;
}

pub trait ClientStore {
    fn load_group_configuration(&self, group_id: Uuid) -> Option<GroupConfiguration>;

    fn save_group_configuration(&mut self, configuration: GroupConfiguration);
}

pub trait CompletionHandler: Send + 'static {
    fn on_commit(&self, user_data: usize);

    fn on_subscription_event(&self, result: &Vec<u8>, user_data: usize);

    fn on_query(&self, result: &Vec<u8>, user_data: usize);
}

pub struct NullCompletionHandler {}

impl CompletionHandler for NullCompletionHandler {
    fn on_commit(&self, _user_data: usize) {}

    fn on_subscription_event(&self, _result: &Vec<u8>, _user_data: usize) {}

    fn on_query(&self, _result: &Vec<u8>, _user_data: usize) {}
}
