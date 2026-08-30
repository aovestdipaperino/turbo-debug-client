# turbo-debug-client

Client for [`turbo-debug-console`](https://crates.io/crates/turbo-debug-console),
a Turbo Vision debug console for live streams.

The console listens on a fixed control port, names your stream, and hands back
a private port to write to. This crate is the twenty lines on the other side of
that exchange, so a program that wants a debug window does not have to depend
on a terminal UI to get one.

```sh
cargo add turbo-debug-client
```

## Streaming text at a window

```rust
use std::io::Write;
use turbo_debug_client::{StreamKind, connect};

let mut sock = connect(StreamKind::Tokens, "build")?;
writeln!(sock, "starting the build")?;
```

The window outlives the socket. Connect again with the same name and you rejoin
the same window, below a `-- reconnected --` rule, with the previous output
still there. Restarting the program you are debugging accumulates runs instead
of losing them, which is the whole reason the handshake carries a name.

## Sending `tracing` records

With the `tracing` feature, records go to a window with their levels coloured,
timestamps dimmed and structured fields shown as `key=value`:

```toml
turbo-debug-client = { version = "0.1", features = ["tracing"] }
```

```rust
use tracing::Level;
use turbo_debug_client::{SocketWriter, StreamKind, connect};

let sock = connect(StreamKind::Trace, "api")?;

tracing_subscriber::fmt()
    .json()
    .with_max_level(Level::TRACE)
    .with_writer(SocketWriter::new(sock))
    .init();

tracing::info!(target: "api::http", method = "GET", status = 200, "request served");
```

Set the max level explicitly. `fmt()` defaults to `INFO`, so without it your
`debug!` and `trace!` calls never reach the socket and it looks like the console
is dropping them.

## The protocol

```
client -> control 7878 :  HELLO 1 <kind> <name>\n
server ->              :  PORT 54312\n     (or  ERR <reason>\n)
client -> data 54312   :  raw bytes, until the socket closes
```

`<kind>` is `tokens` for a model's token stream, rendered as markdown with
syntax highlighting and dimmed thinking text, or `trace` for JSON-lines
`tracing` records. A refused handshake comes back as an `io::Error` carrying
the console's own reason.

Anything that is not a handshake is treated as a raw token stream, so
`cat capture.txt | nc 127.0.0.1 7878` needs no client at all.

## License

MIT
