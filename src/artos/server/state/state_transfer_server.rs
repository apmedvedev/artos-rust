use crate::artos::common::utils::copy;
use crate::artos::common::{Error, ErrorKind};
use crate::artos::server::channel::Event;
use crate::artos::server::state::{
    LogState, StateTransferAcquirer, StateTransferAcquirerFactory, StateTransferFileHeader, StateTransferRequest,
    StateTransferResponseHeader, TransferState,
};
use crate::common::utils::Time;
use crate::compartment::Sender;
use flate2::CrcReader;
use log::debug;
use std::fs::{create_dir_all, remove_dir_all};
use std::io::{BufReader, BufWriter, Seek, SeekFrom};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::thread::{Builder, JoinHandle};
use tempfile::{tempdir, TempDir};

const DEFAULT_BUFFER_SIZE: usize = 8192;

pub struct StateTransferServer {
    handle: JoinHandle<()>,
    address: String,
}

impl StateTransferServer {
    pub fn new(
        endpoint: String,
        port_range_start: Option<u16>,
        port_range_end: Option<u16>,
        work_dir: PathBuf,
        _secured: bool, //TODO: implement SSL/TLS client
        bind_address: Option<String>,
        sender: Sender<Event>,
        state_acquirer_factory: Box<dyn StateTransferAcquirerFactory>,
    ) -> Result<StateTransferServer, Error> {
        let port_start;
        if let Some(port_range_start) = port_range_start {
            port_start = port_range_start;
        } else {
            port_start = 1000;
        }

        let port_end;
        if let Some(port_range_end) = port_range_end {
            port_end = port_range_end;
        } else {
            port_end = 65535;
        }

        let address;
        if let Some(bind_address) = bind_address {
            address = bind_address;
        } else {
            address = hostname::get()?.into_string()?;
        }

        assert!(port_start <= port_end);

        if work_dir.exists() {
            remove_dir_all(&work_dir)?;
        }

        create_dir_all(&work_dir)?;

        let listener = Self::create_socket(&address, port_start, port_end)?;

        let handle = Builder::new().name(String::from("[state_transfer]")).spawn(move || {
            let server =
                StateTransferServerImpl::new(endpoint.clone(), listener, sender.clone(), state_acquirer_factory);
            server.run();
        })?;

        Ok(StateTransferServer {
            handle,
            address: address.clone(),
        })
    }

    fn create_socket(address: &String, port_start: u16, port_end: u16) -> Result<TcpListener, Error> {
        for port in port_start..port_end + 1 {
            let local_address = format!("{}:{}", address.clone(), port);
            match TcpListener::bind(local_address.clone()) {
                Ok(listener) => return Ok(listener),
                Err(_error) => continue,
            }
        }

        Err(Box::new(ErrorKind::CreateTcpListenerError(
            address.clone(),
            port_start,
            port_end,
        )))
    }
}

struct StateTransferServerImpl {
    endpoint: String,
    socket: TcpListener,
    sender: Sender<Event>,
    state_acquirer_factory: Box<dyn StateTransferAcquirerFactory>,
}

impl StateTransferServerImpl {
    fn new(
        endpoint: String,
        socket: TcpListener,
        sender: Sender<Event>,
        state_acquirer_factory: Box<dyn StateTransferAcquirerFactory>,
    ) -> StateTransferServerImpl {
        StateTransferServerImpl {
            endpoint,
            socket,
            sender,
            state_acquirer_factory,
        }
    }

    fn run(&self) {
        loop {
            let result = self.socket.accept();
            match result {
                Ok((socket, _)) => {
                    let handler = StateTransferRequestHandler::new(
                        self.endpoint.clone(),
                        socket,
                        self.state_acquirer_factory.create_acquirer(),
                    );
                    self.sender.execute(move || handler.run())
                }
                Err(_) => break,
            }
        }
    }
}
//////////////////////////////////////////////////////////
// StateTransferRequest
//////////////////////////////////////////////////////////

struct StateTransferRequestHandler {
    endpoint: String,
    socket: TcpStream,
    state_acquirer: Box<dyn StateTransferAcquirer>,
}

impl StateTransferRequestHandler {
    fn new(
        endpoint: String,
        socket: TcpStream,
        state_acquirer: Box<dyn StateTransferAcquirer>,
    ) -> StateTransferRequestHandler {
        StateTransferRequestHandler {
            endpoint,
            socket,
            state_acquirer,
        }
    }

    fn run(&self) {
        debug!(
            "[{}] Request has been accepted from {:?}.",
            self.endpoint,
            self.socket.peer_addr()
        );

        if let Err(error) = self.transfer() {
            debug!("[{}] Error transferring state: {}.", self.endpoint, error);
        }
    }

    fn transfer(&self) -> Result<(), Error> {
        let temp_dir = tempdir()?;
        let request = self.read_request()?;
        let state = self.acquire_state(&request, &temp_dir)?;
        self.write_response(&request, state)?;

        Ok(())
    }

    fn read_request(&self) -> Result<StateTransferRequest, Error> {
        let mut stream = BufReader::new(&self.socket);

        let request: StateTransferRequest = bincode::deserialize_from(&mut stream)?;
        debug!(
            "[{}] State transfer request has been received. Group-id: {}, log-index: {}.",
            self.endpoint,
            request.group_id(),
            request.log_index()
        );

        Ok(request)
    }

    fn acquire_state(&self, request: &StateTransferRequest, temp_dir: &TempDir) -> Result<TransferState, Error> {
        debug!(
            "[{}] Start acquiring state. Group-id: {}, log-index: {}.",
            self.endpoint,
            request.group_id(),
            request.log_index()
        );

        let start = Time::now();

        let state = self
            .state_acquirer
            .acquire_state(request.group_id(), request.log_index(), temp_dir.path())?;

        let delta = Time::now() - start;
        debug!(
            "[{}] End acquiring state. Group-id: {}, log-index: {}, time(ms): {:?}.",
            self.endpoint,
            request.group_id(),
            request.log_index(),
            delta
        );

        Ok(state)
    }

    fn write_response(&self, request: &StateTransferRequest, state: TransferState) -> Result<(), Error> {
        let mut stream = BufWriter::new(&self.socket);

        let mut file_count: u32 = 0;
        if state.snapshot_state().is_some() {
            file_count += 1;
        }

        file_count += state.log_states().len() as u32;

        let header = StateTransferResponseHeader::new(file_count);

        bincode::serialize_into(&mut stream, &header)?;

        if let Some(snapshot_state) = state.snapshot_state() {
            self.transfer_file(snapshot_state, &mut stream, true)?;
        }

        for log_state in state.log_states() {
            self.transfer_file(log_state, &mut stream, false)?;
        }

        debug!(
            "[{}] State transfer request has been completed. Group-id: {}, log-index: {}.",
            self.endpoint,
            request.group_id(),
            request.log_index()
        );

        Ok(())
    }

    fn transfer_file(
        &self,
        state: &LogState,
        mut out_stream: &mut BufWriter<&TcpStream>,
        snapshot: bool,
    ) -> Result<(), Error> {
        let start = Time::now();

        state.file().seek(SeekFrom::Start(0))?;
        let file_size = state.file().metadata()?.len();
        let header = StateTransferFileHeader::new(snapshot, state.is_compressed(), state.log_index(), file_size);

        debug!("[{}] Start transferring file {:?}.", self.endpoint, header);

        bincode::serialize_into(&mut out_stream, &header)?;

        let mut in_stream = CrcReader::new(BufReader::new(state.file()));
        let mut buffer = [0; DEFAULT_BUFFER_SIZE];
        copy(&mut in_stream, &mut out_stream, file_size, &mut buffer)?;

        bincode::serialize_into(out_stream, &in_stream.crc().sum())?;

        let delta = Time::now() - start;
        let rate = file_size as f64 / 1000000 as f64 * 1000 as f64 / delta.as_millis() as f64;
        debug!(
            "[{}] End transferring file {:?}, time(ms): {:?}, rate(mb/s): {}.",
            self.endpoint,
            state.file(),
            delta,
            rate
        );

        Ok(())
    }
}
