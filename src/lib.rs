// Copyright (c) 2026 Enzo Lombardi
// SPDX-License-Identifier: MIT

//! Client for [`turbo-debug-console`], a Turbo Vision debug console.
//!
//! The console listens on a fixed control port, names your stream, and hands
//! back a private port to write to. This crate is the twenty lines on the
//! other side of that exchange, so a program that wants a window does not
//! have to depend on a terminal UI to get one.
//!
//! ```no_run
//! use turbo_debug_client::{StreamKind, connect};
//! use std::io::Write;
//!
//! let mut sock = connect(StreamKind::Tokens, "build")?;
//! writeln!(sock, "starting the build")?;
//! # Ok::<(), std::io::Error>(())
//! ```
//!
//! The window outlives the socket. Connecting again with the same name
//! rejoins the same window, below a `-- reconnected --` rule, with the
//! previous output still there, so restarting the program you are debugging
//! accumulates runs instead of losing them.
//!
//! [`turbo-debug-console`]: https://crates.io/crates/turbo-debug-console

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

/// The console's well-known control port.
pub const CONTROL_PORT: u16 = 7878;

/// The handshake version this client speaks.
pub const PROTOCOL_VERSION: u32 = 1;

/// What the console should make of the bytes you send.
///
/// The kind is part of the handshake because the two are rendered by
/// completely different code paths, and guessing from the first few bytes
/// would be a guess that is wrong exactly when a stream is unusual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    /// A model's token stream: markdown, fenced code, thinking text, tool calls.
    Tokens,
    /// `tracing-subscriber` JSON records, one per line.
    Trace,
}

impl StreamKind {
    /// The wire word for this kind.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tokens => "tokens",
            Self::Trace => "trace",
        }
    }
}

/// Performs the handshake and returns a socket connected to the data port.
///
/// Sends `HELLO <version> <kind> <name>` to the control port and connects to
/// the port the console replies with. The name identifies the stream across
/// restarts: reuse it and you rejoin the same window.
///
/// # Errors
/// Returns the underlying [`std::io::Error`] when the console is not running
/// or the data port cannot be reached, and [`std::io::ErrorKind::InvalidData`]
/// carrying the console's own `ERR ...` text when the handshake is refused,
/// which happens if the name is malformed or the versions disagree.
pub fn connect(kind: StreamKind, name: &str) -> std::io::Result<TcpStream> {
    connect_on(CONTROL_PORT, kind, name)
}

/// Like [`connect`], against a console on a non-default control port.
///
/// The console itself has no option for this; it is here for tests that run
/// a console on an ephemeral port.
///
/// # Errors
/// As [`connect`].
pub fn connect_on(control_port: u16, kind: StreamKind, name: &str) -> std::io::Result<TcpStream> {
    let mut control = TcpStream::connect(("127.0.0.1", control_port))?;
    writeln!(
        control,
        "HELLO {PROTOCOL_VERSION} {} {name}",
        kind.as_str()
    )?;
    control.flush()?;

    let mut reply = String::new();
    BufReader::new(control.try_clone()?).read_line(&mut reply)?;

    let port: u16 = reply
        .trim()
        .strip_prefix("PORT ")
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("handshake refused: {}", reply.trim()),
            )
        })?;

    TcpStream::connect(("127.0.0.1", port))
}

#[cfg(feature = "tracing")]
mod writer;
#[cfg(feature = "tracing")]
pub use writer::SocketWriter;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_use_the_wire_words_the_console_parses() {
        assert_eq!(StreamKind::Tokens.as_str(), "tokens");
        assert_eq!(StreamKind::Trace.as_str(), "trace");
    }

    #[test]
    fn a_refused_handshake_surfaces_the_console_s_own_reason() {
        use std::io::Read;
        use std::net::TcpListener;

        // Stand in for a console that rejects the name.
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let mut sock = listener.incoming().next().unwrap().unwrap();
            let mut buf = [0u8; 128];
            let _ = sock.read(&mut buf);
            let _ = sock.write_all(b"ERR bad name\n");
        });

        let err = connect_on(port, StreamKind::Trace, "x").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(
            err.to_string().contains("ERR bad name"),
            "the console's reason should survive: {err}"
        );
    }

    #[test]
    fn the_handshake_line_is_the_grammar_the_console_expects() {
        use std::net::TcpListener;

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = std::thread::spawn(move || {
            let sock = listener.incoming().next().unwrap().unwrap();
            // Read a whole line rather than one `read`: TCP makes no promise
            // that the handshake arrives in a single segment, and asserting on
            // a partial read is how you get a test that fails on a slow day.
            let mut reader = BufReader::new(sock.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut sock = sock;
            let _ = sock.write_all(b"ERR stop here\n");
            line
        });

        let _ = connect_on(port, StreamKind::Trace, "api");
        assert_eq!(seen.join().unwrap().trim_end(), "HELLO 1 trace api");
    }
}
