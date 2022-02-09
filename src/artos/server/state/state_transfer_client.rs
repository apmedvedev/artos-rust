use crate::artos::common::utils::copy;
use crate::artos::common::{Error, ErrorKind};
use crate::artos::server::state::{
    StateTransferApplier, StateTransferFileHeader, StateTransferRequest, StateTransferResponseHeader,
};
use crate::common::utils::Time;
use flate2::CrcWriter;
use log::debug;
use std::fs::{create_dir_all, remove_dir_all};
use std::io::{BufReader, BufWriter, Seek, SeekFrom};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::tempfile;
use uuid::Uuid;

const DEFAULT_BUFFER_SIZE: usize = 8192;

pub struct StateTransferClient {
    endpoint: String,
    host: String,
    port: u16,
    work_dir: PathBuf,
    secured: bool, //TODO: implement SSL/TLS client
    connect_timeout: Duration,
    read_timeout: Duration,
    write_timeout: Duration,
    state_applier: Box<dyn StateTransferApplier>,
}

impl StateTransferClient {
    pub fn new(
        endpoint: String,
        host: String,
        port: u16,
        work_dir: PathBuf,
        secured: bool,
        connect_timeout: Duration,
        read_timeout: Duration,
        write_timeout: Duration,
        state_applier: Box<dyn StateTransferApplier>,
    ) -> StateTransferClient {
        StateTransferClient {
            endpoint,
            host,
            port,
            work_dir,
            secured,
            connect_timeout,
            read_timeout,
            write_timeout,
            state_applier,
        }
    }

    pub fn transfer(&self, group_id: Uuid, log_index: u64) -> Result<(), Error> {
        debug!(
            "[{}] State transfer request has been sent to server. Group-id: {}, log-index: {}.",
            self.endpoint, group_id, log_index
        );

        if self.work_dir.exists() {
            remove_dir_all(&self.work_dir)?;
        }

        create_dir_all(&self.work_dir)?;

        let socket = self.create_socket()?;

        debug!(
            "[{}] Client has been connected to server. Host: {}, port: {}, secured: {}, work-dir: {:?}.",
            self.endpoint, self.host, self.port, self.secured, self.work_dir
        );

        self.write_request(group_id, log_index, &socket)?;
        self.read_response(&socket)?;

        debug!(
            "[{}] State transfer request has been completed. Group-id: {}, log-index: {}.",
            self.endpoint, group_id, log_index
        );

        Result::Ok(())
    }
}

impl StateTransferClient {
    fn create_socket(&self) -> Result<TcpStream, Error> {
        let address = format!("{}:{}", self.host.clone(), self.port);
        let socket_addr = address.to_socket_addrs()?.next().unwrap();
        let socket = TcpStream::connect_timeout(&socket_addr, self.connect_timeout)?;
        socket.set_read_timeout(Some(self.read_timeout))?;
        socket.set_write_timeout(Some(self.write_timeout))?;

        Ok(socket)
    }

    fn write_request(&self, group_id: Uuid, log_index: u64, socket: &TcpStream) -> Result<(), Error> {
        let mut stream = BufWriter::new(socket);

        let request = StateTransferRequest::new(group_id, log_index);
        bincode::serialize_into(&mut stream, &request)?;

        Ok(())
    }

    fn read_response(&self, socket: &TcpStream) -> Result<(), Error> {
        let mut stream = BufReader::new(socket);
        let header: StateTransferResponseHeader = bincode::deserialize_from(&mut stream)?;

        debug!(
            "[{}] Received files in response: {}.",
            self.endpoint,
            header.file_count()
        );

        for i in 0..header.file_count() {
            self.transfer_file(i, &mut stream)?;
        }

        Ok(())
    }

    fn transfer_file(&self, file_index: u32, mut in_stream: &mut BufReader<&TcpStream>) -> Result<(), Error> {
        let header: StateTransferFileHeader = bincode::deserialize_from(&mut in_stream)?;
        let mut file = tempfile()?;

        debug!(
            "[{}] Start transferring file {}, file-size: {}, snapshot: {}.",
            self.endpoint,
            file_index,
            header.file_size(),
            header.is_snapshot()
        );
        let start = Time::now();
        let mut out_stream = CrcWriter::new(BufWriter::new(&file));
        let mut buffer = [0; DEFAULT_BUFFER_SIZE];

        copy(in_stream, &mut out_stream, header.file_size(), &mut buffer)?;

        let checksum: u32 = bincode::deserialize_from(in_stream)?;
        if out_stream.crc().sum() != checksum {
            return Err(Box::new(ErrorKind::InvalidChecksum));
        }

        drop(out_stream);
        file.seek(SeekFrom::Start(0))?;

        let delta = Time::now() - start;
        let rate = header.file_size() as f64 / 1000000 as f64 * 1000 as f64 / delta.as_millis() as f64;
        debug!(
            "[{}] End transferring file {}, file-size: {}, snapshot: {}, time(ms): {:?}, rate(mb/s): {}.",
            self.endpoint,
            file_index,
            header.file_size(),
            header.is_snapshot(),
            delta,
            rate
        );

        debug!(
            "[{}] Start applying state {}, file-size: {}, snapshot: {}.",
            self.endpoint,
            file_index,
            header.file_size(),
            header.is_snapshot()
        );

        let start = Time::now();

        if header.is_snapshot() {
            self.state_applier
                .apply_snapshot(&file, header.log_index(), header.is_compressed())?;
        } else {
            self.state_applier.apply_log(&file, header.is_compressed())?;
        }

        let delta = Time::now() - start;

        debug!(
            "[{}] End applying state {}, file-size: {}, snapshot: {}, time(ms): {:?}.",
            self.endpoint,
            file_index,
            header.file_size(),
            header.is_snapshot(),
            delta
        );

        Ok(())
    }
}
