use crate::artos::common::Error;
use std::cmp::min;
use std::io::{Read, Write};

pub trait VecExt<V> {
    fn exists<F>(&self, find: F) -> bool
    where
        F: Fn(&V) -> bool;

    fn insert_absent<F>(&mut self, find: F, value: V)
    where
        F: Fn(&V) -> bool;

    fn update_or_insert<F>(&mut self, find: F, value: V)
    where
        F: Fn(&V) -> bool;

    fn remove_exists<F>(&mut self, find: F) -> bool
    where
        F: Fn(&V) -> bool;
}

impl<V> VecExt<V> for Vec<V> {
    fn exists<F>(&self, find: F) -> bool
    where
        F: Fn(&V) -> bool,
    {
        let index = self.iter().position(find);
        index.is_some()
    }

    fn insert_absent<F>(&mut self, find: F, value: V)
    where
        F: Fn(&V) -> bool,
    {
        let index = self.iter().position(find);
        if let None = index {
            self.push(value);
        }
    }

    fn update_or_insert<F>(&mut self, find: F, value: V)
    where
        F: Fn(&V) -> bool,
    {
        let index = self.iter().position(find);
        if let Some(index) = index {
            self[index] = value;
        } else {
            self.push(value);
        }
    }

    fn remove_exists<F>(&mut self, find: F) -> bool
    where
        F: Fn(&V) -> bool,
    {
        let index = self.iter().position(find);
        if let Some(index) = index {
            self.remove(index);
            true
        } else {
            false
        }
    }
}

pub fn copy(
    in_stream: &mut dyn Read,
    out_stream: &mut dyn Write,
    file_size: u64,
    buffer: &mut [u8],
) -> Result<(), Error> {
    let mut file_size = file_size;
    while file_size > 0 {
        let size = min(buffer.len() as u64, file_size) as usize;
        let read_size = in_stream.read(&mut buffer[..size])?;

        out_stream.write_all(&mut buffer[..read_size])?;
        file_size -= read_size as u64;
    }

    Ok(())
}
