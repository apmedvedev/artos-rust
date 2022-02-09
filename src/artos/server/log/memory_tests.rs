#[cfg(test)]
mod tests {
    use super::*;
    use crate::artos::server::test::Setup;
    use crate::common::utils::expect_tracks;
    use crate::common::utils::init_tracks;
    use crate::common::utils::Time;
    use pretty_assertions::assert_eq;
    use std::time::Duration;

    #[test]
    fn test_new() {
        let test = new();

        let state = test.state.lock().unwrap();
        assert_eq!(state.start_index, 1);
        assert_eq!(state.commit_index, 0);
        assert_eq!(state.compaction_locked, false);
    }

    #[test]
    fn test_start_index() {
        let test = new();

        let start_index = 10;
        {
            let mut state = test.state.lock().unwrap();
            state.start_index = start_index;
        }

        assert_eq!(test.start_index(), start_index);
        expect_tracks(vec!["start_index.end"]);
    }

    #[test]
    fn test_end_index() {
        let setup = Setup::new();
        let test = new();

        let start_index = 10;
        {
            let mut state = test.state.lock().unwrap();
            state.start_index = start_index;
        }

        test.append(setup.log_entry.build());

        assert_eq!(test.end_index(), start_index + 1);
        expect_tracks(vec!["append.end", "end_index.end"]);
    }

    #[test]
    fn test_last() {
        let mut setup = Setup::new();
        let test = new();

        let entry1 = setup.log_entry.set_term(1).build();
        let entry2 = setup.log_entry.set_term(2).build();
        test.append(entry1.clone());
        test.append(entry2.clone());

        assert_eq!(test.last(), entry2);
        expect_tracks(vec!["append.end", "append.end", "last.end"]);
    }

    #[test]
    fn test_last_empty() {
        let test = new();

        assert_eq!(test.last(), test.null_log_entry);
        expect_tracks(vec!["last.end_null"]);
    }

    #[test]
    fn test_get() {
        let mut setup = Setup::new();
        let test = new();

        let start_index = 10;
        {
            let mut state = test.state.lock().unwrap();
            state.start_index = start_index;
        }
        let mut entries = vec![];
        for i in 0..10 {
            let entry = setup.log_entry.set_term(i).build();
            test.append(entry.clone());
            entries.push(entry);
        }

        init_tracks();

        assert_eq!(test.get(start_index, start_index + entries.len() as u64), entries);
        assert_eq!(
            test.get(start_index + 1, start_index + entries.len() as u64 - 1),
            &entries[1..entries.len() - 1]
        );
        expect_tracks(vec!["get.end", "get.end"]);
    }

    #[test]
    fn test_get_at() {
        let mut setup = Setup::new();
        let test = new();

        let start_index = 10;
        {
            let mut state = test.state.lock().unwrap();
            state.start_index = start_index;
        }
        let mut entries = vec![];
        for i in 0..10 {
            let entry = setup.log_entry.set_term(i).build();
            test.append(entry.clone());
            entries.push(entry);
        }

        init_tracks();

        assert_eq!(test.get_at(start_index), entries[0]);
        assert_eq!(
            test.get_at(start_index + entries.len() as u64 - 1),
            entries[entries.len() - 1]
        );
        expect_tracks(vec!["get_at.end", "get_at.end"]);
    }

    #[test]
    fn test_commit_index() {
        let test = new();

        let commit_index = 10;
        {
            let mut state = test.state.lock().unwrap();
            state.commit_index = commit_index;
        }

        assert_eq!(test.commit_index(), commit_index);
        expect_tracks(vec!["commit_index.end"]);
    }

    #[test]
    fn test_append() {
        let mut setup = Setup::new();
        let test = new();

        let start_index = 10;
        {
            let mut state = test.state.lock().unwrap();
            state.start_index = start_index;
        }
        let mut entries = vec![];
        for i in 0..10 {
            init_tracks();
            let entry = setup.log_entry.set_term(i).build();
            assert_eq!(test.append(entry.clone()), start_index + i);
            entries.push(entry);
        }

        let state = test.state.lock().unwrap();
        for i in 0..10 {
            assert_eq!(state.entries[i].log_entry, entries[i]);
            assert_eq!(state.entries[i].creation_time, Time::now());
        }
        expect_tracks(vec!["append.end"]);
    }

    #[test]
    fn test_set_at() {
        let mut setup = Setup::new();
        let test = new();

        let start_index = 10;
        {
            let mut state = test.state.lock().unwrap();
            state.start_index = start_index;
        }
        let mut entries = vec![];
        for i in 0..10 {
            let entry = setup.log_entry.set_term(i).build();
            test.append(entry.clone());
            entries.push(entry);
        }

        init_tracks();

        Time::advance(Duration::from_secs(1));

        let entry1 = setup.log_entry.set_term(100).build();
        test.set_at(start_index + 4, entry1.clone());

        let state = test.state.lock().unwrap();
        assert_eq!(state.entries.len(), 5);
        for i in 0..3 {
            assert_eq!(state.entries[i].log_entry, entries[i]);
        }
        assert_eq!(state.entries[4].log_entry, entry1);
        assert_eq!(state.entries[4].creation_time, Time::now());
        expect_tracks(vec!["trim.end", "set_at.end"]);
    }

    #[test]
    fn test_clear() {
        let mut setup = Setup::new();
        let test = new();

        for i in 0..10 {
            let entry = setup.log_entry.set_term(i).build();
            test.append(entry.clone());
        }

        init_tracks();

        let start_index = 100;
        test.clear(start_index);

        let state = test.state.lock().unwrap();
        assert_eq!(state.entries.is_empty(), true);
        assert_eq!(state.start_index, start_index);
        assert_eq!(state.commit_index, start_index);
        expect_tracks(vec!["clear.end"]);
    }

    #[test]
    fn test_commit() {
        let mut setup = Setup::new();
        let test = new();

        for i in 0..10 {
            let entry = setup.log_entry.set_term(i).build();
            test.append(entry.clone());
        }

        init_tracks();

        let commit_index = 10;
        test.commit(commit_index);

        let state = test.state.lock().unwrap();
        assert_eq!(state.commit_index, commit_index);
        expect_tracks(vec!["compact.end", "commit.end"]);
    }

    #[test]
    fn test_read_write() -> Result<()> {
        let mut setup = Setup::new();
        let test = new();

        let start_index = 10;
        {
            let mut state = test.state.lock().unwrap();
            state.start_index = start_index;
        }
        let mut entries = vec![];
        for i in 0..10 {
            let entry = setup.log_entry.set_term(i).set_value(vec![0u8]).build();
            test.append(entry.clone());
            entries.push(entry);
        }

        let mut streams = vec![];
        let mut index = start_index + 5;
        loop {
            let mut stream: Vec<u8> = vec![];
            if let Some(next_index) = test.read(index, &mut stream, 2)? {
                index = next_index;
                streams.push(stream);
            } else {
                streams.push(stream);
                break;
            }
        }

        let test2 = new();
        {
            let mut state = test2.state.lock().unwrap();
            state.start_index = start_index;
        }
        let mut entries2 = vec![];
        for i in 0..10 {
            let entry = setup.log_entry.set_term(i * 10).build();
            test2.append(entry.clone());
            entries2.push(entry);
        }

        for stream in streams {
            test2.write(&mut &stream[..])?;
        }

        assert_eq!(
            test2.get(test2.start_index(), test2.end_index()),
            [&entries2[0..5], &entries[5..]].concat()
        );

        Result::Ok(())
    }

    #[test]
    fn test_compaction_locked() {
        let test = new();

        {
            let mut state = test.state.lock().unwrap();
            state.compaction_locked = true;
        }

        assert_eq!(test.is_compaction_locked(), true);
        expect_tracks(vec!["is_compaction_locked.end"]);
    }

    #[test]
    fn test_lock_compaction() {
        let test = new();

        test.lock_compaction();
        assert_eq!(test.is_compaction_locked(), true);
        expect_tracks(vec!["lock_compaction.end", "is_compaction_locked.end"]);
    }

    #[test]
    fn test_unlock_compaction() {
        let test = new();

        {
            let mut state = test.state.lock().unwrap();
            state.compaction_locked = true;
        }

        test.unlock_compaction();
        assert_eq!(test.is_compaction_locked(), false);
        expect_tracks(vec![
            "compact.locked_or_empty",
            "unlock_compaction.end",
            "is_compaction_locked.end",
        ]);
    }

    #[test]
    fn test_trim() {
        let mut setup = Setup::new();
        let test = new();

        let start_index = 10;
        {
            let mut state = test.state.lock().unwrap();
            state.start_index = start_index;
        }

        let mut entries = vec![];
        for i in 0..10 {
            let entry = setup.log_entry.set_term(i).build();
            test.append(entry.clone());
            entries.push(entry);
        }

        init_tracks();

        {
            let mut state = test.state.lock().unwrap();
            test.trim(start_index + 5, &mut state);
        }

        assert_eq!(test.get(start_index, start_index + 5), &entries[0..5]);
        expect_tracks(vec!["trim.end", "get.end"]);
    }

    #[test]
    fn test_compact_locked_or_empty() {
        let mut setup = Setup::new();
        let test = new();

        {
            let mut state = test.state.lock().unwrap();
            test.compact(&mut state);

            state.compaction_locked = true;
        }

        test.append(setup.log_entry.set_term(1).build());

        {
            let mut state = test.state.lock().unwrap();
            test.compact(&mut state);
        }

        expect_tracks(vec!["compact.locked_or_empty", "append.end", "compact.locked_or_empty"]);
    }

    #[test]
    fn test_compact_by_commit_index() {
        let mut setup = Setup::new();
        let test = new();

        let start_index = 10;
        {
            let mut state = test.state.lock().unwrap();
            state.start_index = start_index;
            state.commit_index = start_index + 5;
        }
        let mut entries = vec![];
        for i in 0..10 {
            let entry = setup.log_entry.set_term(i).build();
            test.append(entry.clone());
            entries.push(entry);
        }

        init_tracks();
        Time::advance(Duration::from_secs(100));

        {
            let mut state = test.state.lock().unwrap();
            test.compact(&mut state)
        }

        expect_tracks(vec!["compact.end"]);

        assert_eq!(test.start_index(), start_index + 5);
        assert_eq!(
            test.get(test.start_index(), test.end_index()),
            &entries[5..entries.len()]
        );
    }

    #[test]
    fn test_compact_by_time() {
        let mut setup = Setup::new();
        let test = new();

        let start_index = 10;
        {
            let mut state = test.state.lock().unwrap();
            state.start_index = start_index;
            state.commit_index = start_index + 100;
        }
        let mut entries = vec![];
        for i in 0..10 {
            if i == 5 {
                Time::advance(Duration::from_secs(100));
            }

            let entry = setup.log_entry.set_term(i).build();
            test.append(entry.clone());
            entries.push(entry);
        }

        init_tracks();

        {
            let mut state = test.state.lock().unwrap();
            test.compact(&mut state)
        }

        expect_tracks(vec!["compact.end"]);

        assert_eq!(test.start_index(), start_index + 5);
        assert_eq!(
            test.get(test.start_index(), test.end_index()),
            &entries[5..entries.len()]
        );
    }

    fn new() -> MemoryLogStore {
        Setup::init_time();
        MemoryLogStore::new(Duration::from_secs(10))
    }
}
