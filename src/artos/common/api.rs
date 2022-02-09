use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fmt::Debug;
use std::net::AddrParseError;
use std::{fmt, io};
use uuid::Uuid;

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfiguration {
    id: Uuid,
    endpoint: String,
}

impl ServerConfiguration {
    pub fn new(id: Uuid, endpoint: String) -> ServerConfiguration {
        ServerConfiguration { id, endpoint }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn endpoint(&self) -> &String {
        &self.endpoint
    }
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct GroupConfiguration {
    name: String,
    id: Uuid,
    servers: Vec<ServerConfiguration>,
    single_server: bool,
}

impl GroupConfiguration {
    pub fn new(name: String, id: Uuid, servers: Vec<ServerConfiguration>, single_server: bool) -> GroupConfiguration {
        if single_server {
            assert_eq!(servers.len(), 1);
        } else {
            assert!(!servers.is_empty());
        }

        let mut servers = servers;
        servers.dedup_by(|x, y| x.id == y.id);

        GroupConfiguration {
            name,
            id,
            servers,
            single_server,
        }
    }

    pub fn name(&self) -> &String {
        &self.name
    }
    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn servers(&self) -> &Vec<ServerConfiguration> {
        &self.servers
    }

    pub fn is_single_server(&self) -> bool {
        self.single_server
    }
}

#[cfg_attr(test, mockall::automock)]
pub trait MessageSenderConfiguration: Send + 'static {
    fn build(&self) -> Box<dyn MessageSenderFactory>;
}

#[cfg_attr(test, mockall::automock)]
pub trait MessageSenderFactory: Send + 'static {
    fn create_sender(
        &self,
        endpoint: String,
        response_handler: Box<dyn ResponseHandler>,
    ) -> Result<Box<dyn MessageSender>>;
}

#[cfg_attr(test, mockall::automock)]
pub trait MessageSender: Send + 'static {
    fn send(&self, request: Vec<u8>);
}

#[cfg_attr(test, mockall::automock)]
pub trait ResponseHandler: Send + Sync + 'static {
    fn on_succeeded(&self, result: Vec<u8>);

    fn on_failed(&self, error: Error);
}

pub enum FlowType {
    Publish,
    Query,
}

#[cfg_attr(test, mockall::automock)]
pub trait FlowController: Send + 'static {
    fn lock_flow(&self, flow: FlowType);

    fn unlock_flow(&self, flow: FlowType);
}

pub type Result<T> = ::std::result::Result<T, Error>;
pub type Error = Box<ErrorKind>;

#[derive(Debug)]
pub enum ErrorKind {
    Io(io::Error),
    Serialize(bincode::Error),
    SocketAddrParseError(AddrParseError),
    InvalidGroupId(Uuid),
    MaxValueSizeExceeded(usize),
    ServerNotLeader(Uuid),
    SingleServerGroup,
    TimeoutOccurred,
    InvalidChecksum,
    InvalidEncoding(OsString),
    CreateTcpListenerError(String, u16, u16),
}

impl std::error::Error for ErrorKind {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            ErrorKind::Io(ref err) => Some(err),
            ErrorKind::Serialize(ref err) => Some(err),
            ErrorKind::SocketAddrParseError(ref err) => Some(err),
            ErrorKind::InvalidGroupId(_) => None,
            ErrorKind::MaxValueSizeExceeded(_) => None,
            ErrorKind::ServerNotLeader(_) => None,
            ErrorKind::SingleServerGroup => None,
            ErrorKind::TimeoutOccurred => None,
            ErrorKind::InvalidChecksum => None,
            ErrorKind::InvalidEncoding(_) => None,
            ErrorKind::CreateTcpListenerError(_, _, _) => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        ErrorKind::Io(err).into()
    }
}

impl From<bincode::Error> for Error {
    fn from(err: bincode::Error) -> Error {
        ErrorKind::Serialize(err).into()
    }
}

impl From<AddrParseError> for Error {
    fn from(err: AddrParseError) -> Error {
        ErrorKind::SocketAddrParseError(err).into()
    }
}

impl From<OsString> for Error {
    fn from(err: OsString) -> Error {
        ErrorKind::InvalidEncoding(err).into()
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ErrorKind::Io(ref err) => write!(fmt, "io error: {}", err),
            ErrorKind::Serialize(ref err) => write!(fmt, "serialize error: {}", err),
            ErrorKind::SocketAddrParseError(ref err) => write!(fmt, "socket address parse error: {}", err),
            ErrorKind::InvalidGroupId(ref id) => write!(fmt, "invalid group id specified: {}", id),
            ErrorKind::MaxValueSizeExceeded(ref size) => write!(fmt, "maximum value size specified: {}", size),
            ErrorKind::ServerNotLeader(ref id) => write!(fmt, "server is not a leader: {}", id),
            ErrorKind::SingleServerGroup => write!(fmt, "group consists of a single server"),
            ErrorKind::TimeoutOccurred => write!(fmt, "timeout occurred"),
            ErrorKind::InvalidChecksum => write!(fmt, "invalid checksum"),
            ErrorKind::InvalidEncoding(ref err) => write!(fmt, "invalid encoding: {:?}", err),
            ErrorKind::CreateTcpListenerError(ref bind_address, port_range_start, port_range_end) => write!(
                fmt,
                "create tcp listener error, bind_address: {}:[{},{}]",
                bind_address, port_range_start, port_range_end
            ),
        }
    }
}
