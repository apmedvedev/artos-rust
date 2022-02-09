#[cfg(test)]
mod tests {
    use crate::artos::common::Error;
    use crate::artos::server::channel::Event;
    use crate::artos::server::state::state_transfer_client::StateTransferClient;
    use crate::artos::server::state::state_transfer_server::StateTransferServer;
    use crate::artos::server::state::{
        LogState, StateTransferAcquirer, StateTransferAcquirerFactory, StateTransferApplier, TransferState,
    };
    use crate::common::meter::Meter;
    use crate::compartment::{Compartment, Delayed, Handler, HandlerFactory, Sender};
    use std::fs::File;
    use std::io::{BufReader, BufWriter, Read, Write};
    use std::path::Path;
    use std::time::{Duration, Instant};
    use tempfile::{tempdir, tempfile};
    use uuid::Uuid;

    struct TestStateTransferApplier {
        count: usize,
    }

    impl TestStateTransferApplier {
        fn new(count: usize) -> TestStateTransferApplier {
            TestStateTransferApplier { count }
        }

        fn check(&self, file: &File, offset: usize) -> Result<(), Error> {
            let mut stream = BufReader::new(file);

            let mut buffer = [0u8; 100];
            for _ in 0..self.count {
                stream.read_exact(&mut buffer)?;

                for i in 0..buffer.len() {
                    if buffer[i] != (i + offset) as u8 {
                        panic!();
                    }
                }
            }

            Ok(())
        }
    }

    impl StateTransferApplier for TestStateTransferApplier {
        fn apply_snapshot(&self, state_file: &File, log_index: u64, _compressed: bool) -> Result<(), Error> {
            self.check(state_file, 10)?;
            if log_index != 100 {
                panic!();
            }

            Ok(())
        }

        fn apply_log(&self, state_file: &File, _compressed: bool) -> Result<(), Error> {
            self.check(state_file, 100)?;

            Ok(())
        }
    }

    struct TestStateTransferAcquirer {
        count: usize,
    }

    impl TestStateTransferAcquirer {
        fn new(count: usize) -> TestStateTransferAcquirer {
            TestStateTransferAcquirer { count }
        }

        fn generate(&self, file: &File, offset: usize) -> Result<(), Error> {
            let mut buffer = [0u8; 100];
            for i in 0..100 {
                buffer[i] = (i + offset) as u8;
            }

            let mut stream = BufWriter::new(file);
            for _ in 0..self.count {
                stream.write_all(&buffer)?;
            }

            Ok(())
        }
    }

    impl StateTransferAcquirer for TestStateTransferAcquirer {
        fn acquire_state(&self, _group_id: Uuid, _log_index: u64, _work_dir: &Path) -> Result<TransferState, Error> {
            let snapshot_file = tempfile()?;
            self.generate(&snapshot_file, 10)?;

            let log_file1 = tempfile()?;
            self.generate(&log_file1, 100)?;

            let log_file2 = tempfile()?;
            self.generate(&log_file2, 100)?;

            Ok(TransferState::new(
                Some(LogState::new(snapshot_file, false, 100)),
                vec![LogState::new(log_file1, false, 0), LogState::new(log_file2, false, 0)],
            ))
        }
    }

    struct TestStateTransferAcquirerFactory {
        count: usize,
    }

    impl TestStateTransferAcquirerFactory {
        fn new(count: usize) -> TestStateTransferAcquirerFactory {
            TestStateTransferAcquirerFactory { count }
        }
    }

    impl StateTransferAcquirerFactory for TestStateTransferAcquirerFactory {
        fn create_acquirer(&self) -> Box<dyn StateTransferAcquirer> {
            Box::new(TestStateTransferAcquirer::new(self.count))
        }
    }

    struct TestHandler {}

    impl TestHandler {
        fn new(_sender: Sender<Event>, _delayed: Delayed<Event>) -> TestHandler {
            TestHandler {}
        }
    }

    impl Handler<Event> for TestHandler {
        fn process(&self) {}

        fn on_timer(&self, _current_time: Instant) {}

        fn handle(&self, _event: Event) {}
    }

    struct TestHandlerFactory {}

    impl TestHandlerFactory {
        fn new() -> TestHandlerFactory {
            TestHandlerFactory {}
        }
    }

    impl HandlerFactory<TestHandler, Event> for TestHandlerFactory {
        fn build(self, sender: Sender<Event>, delayed: Delayed<Event>) -> TestHandler {
            TestHandler::new(sender, delayed)
        }
    }

    #[test]
    fn test_client_server() -> Result<(), Error> {
        test(1000)
    }

    #[ignore]
    #[test]
    fn perf_test_client_server() -> Result<(), Error> {
        test(10_000_000)
    }

    fn test(count: usize) -> Result<(), Error> {
        let server_work_dir = tempdir()?.into_path();
        let client_work_dir = tempdir()?.into_path();
        let state_acquirer_factory = Box::new(TestStateTransferAcquirerFactory::new(count));
        let state_applier = Box::new(TestStateTransferApplier::new(count));

        let compartment = Compartment::with_config(TestHandlerFactory::new(), 100, Duration::from_millis(100), 4);

        let _server = StateTransferServer::new(
            String::from("test"),
            Some(17171),
            Some(17171),
            server_work_dir,
            false,
            Some(String::from("127.0.0.1")),
            compartment.sender().clone(),
            state_acquirer_factory,
        );

        let client = StateTransferClient::new(
            String::from("test"),
            String::from("127.0.0.1"),
            17171,
            client_work_dir,
            false,
            Duration::from_secs(10),
            Duration::from_secs(100),
            Duration::from_secs(100),
            state_applier,
        );

        let _meter = Meter::new("State transfer time");
        client.transfer(Uuid::new_v4(), 10)?;

        Ok(())
    }
}
