use crate::artos::common::Result;
use crate::artos::message::server::{LogEntry, LogValueType, NULL_UUID};
use crate::artos::server::api::LogStore;
use crate::common::utils::Time;
use crate::track;
use crate::track_method;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::Mutex;
use std::time::{Duration, Instant};

include!("memory_tests.rs");

pub struct MemoryLogStore {
    null_log_entry: LogEntry,
    min_retention_period: Duration,
    state: Mutex<State>,
}

struct State {
    start_index: u64,
    entries: VecDeque<EntryInfo>,
    commit_index: u64,
    compaction_locked: bool,
}

struct EntryInfo {
    log_entry: LogEntry,
    creation_time: Instant,
}

impl MemoryLogStore {
    pub fn new(min_retention_period: Duration) -> MemoryLogStore {
        MemoryLogStore {
            null_log_entry: LogEntry::new(0, vec![], LogValueType::Application, NULL_UUID, 0),
            min_retention_period,
            state: Mutex::new(State {
                start_index: 1,
                entries: VecDeque::new(),
                commit_index: 0,
                compaction_locked: false,
            }),
        }
    }
}

impl LogStore for MemoryLogStore {
    fn start_index(&self) -> u64 {
        track_method!(t, "start_index");

        let state = &self.state.lock().unwrap();

        track!(t, "end");

        state.start_index
    }

    fn end_index(&self) -> u64 {
        track_method!(t, "end_index");

        let state = &self.state.lock().unwrap();

        track!(t, "end");

        state.start_index + state.entries.len() as u64
    }

    fn last(&self) -> LogEntry {
        track_method!(t, "last");

        let state = &self.state.lock().unwrap();
        if !state.entries.is_empty() {
            track!(t, "end");

            state.entries[state.entries.len() - 1].log_entry.clone()
        } else {
            track!(t, "end_null");

            self.null_log_entry.clone()
        }
    }

    fn get(&self, start_log_index: u64, end_log_index: u64) -> Vec<LogEntry> {
        track_method!(t, "get");

        let state = &self.state.lock().unwrap();
        assert!(
            start_log_index <= end_log_index
                && start_log_index >= state.start_index
                && end_log_index <= state.start_index + state.entries.len() as u64
        );

        let start_index = (start_log_index - state.start_index) as usize;
        let end_index = (end_log_index - state.start_index) as usize;

        let mut log_entries = vec![];
        for i in start_index..end_index {
            log_entries.push(state.entries[i].log_entry.clone());
        }

        track!(t, "end");

        log_entries
    }

    fn get_at(&self, log_index: u64) -> LogEntry {
        track_method!(t, "get_at");

        let state = &self.state.lock().unwrap();
        assert!(log_index >= state.start_index && log_index < state.start_index + state.entries.len() as u64);

        let i = (log_index - state.start_index) as usize;

        track!(t, "end");

        state.entries[i].log_entry.clone()
    }

    fn commit_index(&self) -> u64 {
        track_method!(t, "commit_index");

        let state = &self.state.lock().unwrap();

        track!(t, "end");

        state.commit_index
    }

    fn append(&self, log_entry: LogEntry) -> u64 {
        track_method!(t, "append");

        let state = &mut self.state.lock().unwrap();
        state.entries.push_back(EntryInfo {
            log_entry,
            creation_time: Time::now(),
        });

        track!(t, "end");

        state.start_index + state.entries.len() as u64 - 1
    }

    fn set_at(&self, log_index: u64, log_entry: LogEntry) {
        track_method!(t, "set_at");

        let state = &mut self.state.lock().unwrap();
        assert!(log_index >= state.start_index && log_index <= state.start_index + state.entries.len() as u64);

        self.trim(log_index, state);
        state.entries.push_back(EntryInfo {
            log_entry,
            creation_time: Time::now(),
        });

        track!(t, "end");
    }

    fn clear(&self, start_log_index: u64) {
        track_method!(t, "clear");

        let state = &mut self.state.lock().unwrap();
        state.entries.clear();
        state.start_index = start_log_index;
        state.commit_index = start_log_index;

        track!(t, "end");
    }

    fn commit(&self, log_index: u64) {
        track_method!(t, "commit");

        let state = &mut self.state.lock().unwrap();
        assert!(log_index >= state.start_index && log_index < state.start_index + state.entries.len() as u64);

        state.commit_index = log_index;
        self.compact(state);

        track!(t, "end");
    }

    fn read(&self, start_log_index: u64, mut stream: &mut dyn Write, max_size: usize) -> Result<Option<u64>> {
        track_method!(t, "read");

        let state = &mut self.state.lock().unwrap();
        assert!(
            start_log_index >= state.start_index && start_log_index <= state.start_index + state.entries.len() as u64
        );

        let mut size = 0;
        let mut log_entries = vec![];
        let mut log_index = (start_log_index - state.start_index) as usize;
        while log_index < state.entries.len() {
            let log_entry = &state.entries[log_index].log_entry;

            size += log_entry.value().len();

            if size > max_size {
                break;
            }

            log_entries.push(log_entry.clone());
            log_index += 1;
        }

        bincode::serialize_into(&mut stream, &start_log_index)?;

        for log_entry in log_entries {
            bincode::serialize_into(&mut stream, &true)?;
            bincode::serialize_into(&mut stream, &log_entry)?;
        }

        bincode::serialize_into(&mut stream, &false)?;

        track!(t, "end");

        if log_index >= state.entries.len() {
            Result::Ok(None)
        } else {
            Result::Ok(Some(log_index as u64 + state.start_index))
        }
    }

    fn write(&self, mut stream: &mut dyn Read) -> Result<()> {
        track_method!(t, "write");

        let state = &mut self.state.lock().unwrap();
        let start_log_index: u64 = bincode::deserialize_from(&mut stream)?;
        let mut log_entries = vec![];
        while bincode::deserialize_from::<_, bool>(&mut stream)? {
            let log_entry: LogEntry = bincode::deserialize_from(&mut stream)?;
            log_entries.push(log_entry);
        }

        assert!(start_log_index >= state.start_index);
        assert!(start_log_index <= state.start_index + state.entries.len() as u64);
        self.trim(start_log_index, state);

        for log_entry in log_entries {
            state.entries.push_back(EntryInfo {
                log_entry,
                creation_time: Time::now(),
            });
        }

        track!(t, "end");

        Result::Ok(())
    }

    fn is_compaction_locked(&self) -> bool {
        track_method!(t, "is_compaction_locked");

        let state = &mut self.state.lock().unwrap();

        track!(t, "end");

        state.compaction_locked
    }

    fn lock_compaction(&self) {
        track_method!(t, "lock_compaction");

        let state = &mut self.state.lock().unwrap();
        state.compaction_locked = true;

        track!(t, "end");
    }

    fn unlock_compaction(&self) {
        track_method!(t, "unlock_compaction");

        let state = &mut self.state.lock().unwrap();
        state.compaction_locked = false;
        self.compact(state);

        track!(t, "end");
    }
}

impl MemoryLogStore {
    fn trim(&self, log_index: u64, state: &mut State) {
        track_method!(t, "trim");

        let remove_count = state.entries.len() - (log_index - state.start_index) as usize;
        for _ in 0..remove_count {
            state.entries.pop_back();
        }

        track!(t, "end");
    }

    fn compact(&self, state: &mut State) {
        track_method!(t, "compact");

        if state.compaction_locked || state.entries.is_empty() {
            track!(t, "locked_or_empty");
            return;
        }

        let current_time = Time::now();
        while state.commit_index - state.start_index > 0 {
            if let Some(info) = state.entries.front() {
                if self.min_retention_period.as_millis() == 0
                    || current_time > info.creation_time + self.min_retention_period
                {
                    state.entries.pop_front();
                    state.start_index += 1;
                } else {
                    break;
                }
            }
        }

        track!(t, "end");
    }
}
