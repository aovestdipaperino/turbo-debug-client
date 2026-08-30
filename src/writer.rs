// Copyright (c) 2026 Enzo Lombardi
// SPDX-License-Identifier: MIT

//! A `tracing-subscriber` writer that sends records to the console.

use std::io::Write;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

/// Wraps a console socket so `tracing-subscriber` can write to it.
///
/// `tracing-subscriber`'s JSON layer emits one record per line, which is
/// exactly what the console's `trace` kind expects, so there is no formatting
/// glue here: this only makes the socket shareable, since `MakeWriter` hands
/// out a fresh writer per event and events can arrive from several threads.
#[derive(Debug, Clone)]
pub struct SocketWriter(Arc<Mutex<TcpStream>>);

impl SocketWriter {
    /// Wraps a socket, normally the one [`crate::connect`] returned.
    #[must_use]
    pub fn new(socket: TcpStream) -> Self {
        Self(Arc::new(Mutex::new(socket)))
    }
}

impl From<TcpStream> for SocketWriter {
    fn from(socket: TcpStream) -> Self {
        Self::new(socket)
    }
}

impl Write for SocketWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // A poisoned lock means another thread panicked mid-write. Losing a
        // log line is better than panicking inside the logger and turning a
        // crash into two crashes, so take the data and carry on.
        match self.0.lock() {
            Ok(mut sock) => sock.write(buf),
            Err(poisoned) => poisoned.into_inner().write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self.0.lock() {
            Ok(mut sock) => sock.flush(),
            Err(poisoned) => poisoned.into_inner().flush(),
        }
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SocketWriter {
    type Writer = SocketWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}
