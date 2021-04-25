#![cfg(feature = "leveldb")]

use crate::event::Event;
use futures::{Sink, Stream};
use pin_project::pin_project;
use snafu::Snafu;
use std::{
    io,
    path::{Path, PathBuf},
    pin::Pin,
    task::{Context, Poll},
};

pub mod leveldb_buffer;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("The configured data_dir {:?} does not exist, please create it and make sure the vector process can write to it", data_dir))]
    DataDirNotFound { data_dir: PathBuf },
    #[snafu(display("The configured data_dir {:?} is not writable by the vector process, please ensure vector can write to that directory", data_dir))]
    DataDirNotWritable { data_dir: PathBuf },
    #[snafu(display("Unable to look up data_dir {:?}", data_dir))]
    DataDirMetadataError {
        data_dir: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("Unable to open data_dir {:?}", data_dir))]
    DataDirOpenError {
        data_dir: PathBuf,
        source: leveldb::database::error::Error,
    },
}

pub trait DiskBuffer {
    type Writer: Sink<Event, Error = ()>;
    type Reader: Stream<Item = Event> + Send;

    fn build(
        path: PathBuf,
        max_size: usize,
    ) -> Result<(Self::Writer, Self::Reader, super::Acker), Error>;
}

#[pin_project]
#[derive(Clone)]
pub struct Writer {
    #[pin]
    inner: leveldb_buffer::Writer,
}

impl Sink<Event> for Writer {
    type Error = ();
    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_ready(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Event) -> Result<(), Self::Error> {
        self.project().inner.start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_close(cx)
    }
}

pub fn open(
    data_dir: &Path,
    name: &str,
    max_size: usize,
) -> Result<(Writer, Box<dyn Stream<Item = Event> + Send>, super::Acker), Error> {
    let path = data_dir.join(name);

    // Check data dir
    std::fs::metadata(&data_dir)
        .map_err(|e| match e.kind() {
            io::ErrorKind::PermissionDenied => Error::DataDirNotWritable {
                data_dir: data_dir.into(),
            },
            io::ErrorKind::NotFound => Error::DataDirNotFound {
                data_dir: data_dir.into(),
            },
            _ => Error::DataDirMetadataError {
                data_dir: data_dir.into(),
                source: e,
            },
        })
        .and_then(|m| {
            if m.permissions().readonly() {
                Err(Error::DataDirNotWritable {
                    data_dir: data_dir.into(),
                })
            } else {
                Ok(())
            }
        })?;

    let (writer, reader, acker) = leveldb_buffer::Buffer::build(path, max_size)?;
    Ok((Writer { inner: writer }, Box::new(reader), acker))
}
