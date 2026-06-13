//! Phase 5.2 — local IPC transport between the main app and the
//! crash-isolated `sundaystage-output` processes.
//!
//! Carries the existing [`OutputMessage`]/[`OutputAck`] wire protocol as
//! newline-delimited JSON over a Unix domain socket (macOS/Linux) or a named
//! pipe (Windows). The framing layer is byte-stream agnostic — any
//! `AsyncRead`/`AsyncWrite` works — so the protocol round-trip, partial-read
//! and disconnect behaviour are all unit-testable headlessly (with an
//! in-memory duplex), while the OS endpoints are exercised by the
//! `output_isolation` integration test against the real binary.
//!
//! Layout of a frame: one JSON document, one `\n`. Blank lines are ignored so
//! a stray newline can never kill the link mid-service.

use std::io;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

/// Both halves of a byte stream we can frame messages over.
pub trait AsyncStream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> AsyncStream for T {}

/// A connected, transport-agnostic stream (Unix socket, named pipe, or an
/// in-memory duplex in tests).
pub struct IpcStream {
    inner: Box<dyn AsyncStream>,
}

impl IpcStream {
    pub fn new(inner: impl AsyncStream + 'static) -> Self {
        Self {
            inner: Box::new(inner),
        }
    }

    /// Split into a framed reader + writer pair for use from separate tasks.
    pub fn into_split(self) -> (FrameReader, FrameWriter) {
        let (r, w) = tokio::io::split(self.inner);
        (
            FrameReader {
                inner: BufReader::new(Box::new(r) as Box<dyn AsyncReadSend>),
            },
            FrameWriter { inner: Box::new(w) },
        )
    }
}

/// Object-safe read half (`tokio::io::split` returns generic halves).
pub trait AsyncReadSend: AsyncRead + Send + Unpin {}
impl<T: AsyncRead + Send + Unpin> AsyncReadSend for T {}
/// Object-safe write half.
pub trait AsyncWriteSend: AsyncWrite + Send + Unpin {}
impl<T: AsyncWrite + Send + Unpin> AsyncWriteSend for T {}

/// Reads newline-delimited JSON frames. `read()` returns `Ok(None)` on a clean
/// EOF (peer closed the link — i.e. it crashed or shut down).
pub struct FrameReader {
    inner: BufReader<Box<dyn AsyncReadSend>>,
}

impl FrameReader {
    /// Next decoded frame, `None` on EOF. Blank lines are skipped; a malformed
    /// line is an error the caller can surface without dropping the link.
    pub async fn read<T: DeserializeOwned>(&mut self) -> io::Result<Option<T>> {
        let mut line = String::new();
        loop {
            line.clear();
            let n = self.inner.read_line(&mut line).await?;
            if n == 0 {
                return Ok(None); // EOF — peer is gone.
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue; // tolerate stray newlines
            }
            return serde_json::from_str(trimmed)
                .map(Some)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
        }
    }
}

/// Writes newline-delimited JSON frames.
pub struct FrameWriter {
    inner: Box<dyn AsyncWriteSend>,
}

impl FrameWriter {
    pub async fn write<T: Serialize>(&mut self, msg: &T) -> io::Result<()> {
        let mut buf =
            serde_json::to_vec(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        buf.push(b'\n');
        self.inner.write_all(&buf).await?;
        self.inner.flush().await
    }
}

// ── OS endpoints ─────────────────────────────────────────────────────────────

/// The endpoint path for an output process identified by `tag` (its window
/// label). Unix: a socket file in the system temp dir. Windows: a named pipe.
/// Deterministic per tag so a relaunched main app rebinds the same endpoint.
pub fn endpoint_path(tag: &str) -> PathBuf {
    // Keep only filesystem-safe characters; labels are `output-<role>-<idx>`.
    let safe: String = tag
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    #[cfg(unix)]
    {
        // Unix socket paths are length-limited (~104 bytes on macOS); the
        // system temp dir is comfortably short.
        std::env::temp_dir().join(format!("sundaystage-{safe}.sock"))
    }
    #[cfg(windows)]
    {
        PathBuf::from(format!(r"\\.\pipe\sundaystage-{safe}"))
    }
}

/// A bound listener the main app accepts output-process connections on.
pub struct IpcListener {
    #[cfg(unix)]
    inner: tokio::net::UnixListener,
    #[cfg(windows)]
    path: String,
    #[cfg(windows)]
    next: Option<tokio::net::windows::named_pipe::NamedPipeServer>,
    /// Remembered so `Drop` can remove the socket file (unix).
    #[cfg(unix)]
    path: PathBuf,
}

impl IpcListener {
    /// Bind the endpoint, replacing any stale one from a crashed predecessor.
    pub fn bind(path: &Path) -> io::Result<Self> {
        #[cfg(unix)]
        {
            // A previous (crashed) run leaves the socket file behind; remove it
            // so rebinding the deterministic path always works.
            let _ = std::fs::remove_file(path);
            Ok(Self {
                inner: tokio::net::UnixListener::bind(path)?,
                path: path.to_path_buf(),
            })
        }
        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ServerOptions;
            let path_str = path.to_string_lossy().to_string();
            let first = ServerOptions::new()
                .first_pipe_instance(true)
                .create(&path_str)?;
            Ok(Self {
                path: path_str,
                next: Some(first),
            })
        }
    }

    /// Wait for the next output-process connection.
    pub async fn accept(&mut self) -> io::Result<IpcStream> {
        #[cfg(unix)]
        {
            let (stream, _) = self.inner.accept().await?;
            Ok(IpcStream::new(stream))
        }
        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ServerOptions;
            let server = match self.next.take() {
                Some(s) => s,
                None => ServerOptions::new().create(&self.path)?,
            };
            server.connect().await?;
            // Pre-create the next instance so a reconnecting child never races
            // a missing pipe.
            self.next = ServerOptions::new().create(&self.path).ok();
            Ok(IpcStream::new(server))
        }
    }
}

#[cfg(unix)]
impl Drop for IpcListener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Connect to the main app's endpoint (called from the output process).
pub async fn connect(path: &Path) -> io::Result<IpcStream> {
    #[cfg(unix)]
    {
        let stream = tokio::net::UnixStream::connect(path).await?;
        Ok(IpcStream::new(stream))
    }
    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ClientOptions;
        let path_str = path.to_string_lossy().to_string();
        loop {
            match ClientOptions::new().open(&path_str) {
                Ok(client) => return Ok(IpcStream::new(client)),
                // All pipe instances busy — retry shortly (documented dance).
                Err(e) if e.raw_os_error() == Some(231) => {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{OutputAck, OutputMessage};
    use crate::services::live_session::LiveFrame;

    #[tokio::test]
    async fn frames_round_trip_over_a_duplex() {
        let (a, b) = tokio::io::duplex(1024);
        let (mut ra, mut wa) = IpcStream::new(a).into_split();
        let (mut rb, mut wb) = IpcStream::new(b).into_split();

        let msg = OutputMessage::Render {
            frame: LiveFrame::Black,
            seq: 7,
        };
        wa.write(&msg).await.unwrap();
        let got: OutputMessage = rb.read().await.unwrap().expect("frame");
        assert_eq!(got, msg);

        // And back the other way (ack path).
        let ack = OutputAck::Rendered {
            seq: 7,
            rendered_at: 123,
        };
        wb.write(&ack).await.unwrap();
        let got: OutputAck = ra.read().await.unwrap().expect("ack");
        assert_eq!(got, ack);
    }

    #[tokio::test]
    async fn partial_writes_are_reassembled_into_one_frame() {
        use tokio::io::AsyncWriteExt as _;
        let (mut a, b) = tokio::io::duplex(1024);
        let (mut rb, _wb) = IpcStream::new(b).into_split();

        let json = serde_json::to_string(&OutputMessage::Heartbeat { at: 42 }).unwrap();
        let (left, right) = json.split_at(json.len() / 2);

        let reader = tokio::spawn(async move { rb.read::<OutputMessage>().await });
        a.write_all(left.as_bytes()).await.unwrap();
        a.flush().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        a.write_all(right.as_bytes()).await.unwrap();
        a.write_all(b"\n").await.unwrap();
        a.flush().await.unwrap();

        let got = reader.await.unwrap().unwrap().expect("assembled frame");
        assert_eq!(got, OutputMessage::Heartbeat { at: 42 });
    }

    #[tokio::test]
    async fn blank_lines_are_skipped_not_fatal() {
        use tokio::io::AsyncWriteExt as _;
        let (mut a, b) = tokio::io::duplex(1024);
        let (mut rb, _wb) = IpcStream::new(b).into_split();
        a.write_all(b"\n  \n").await.unwrap();
        let json = serde_json::to_string(&OutputMessage::Shutdown).unwrap();
        a.write_all(json.as_bytes()).await.unwrap();
        a.write_all(b"\n").await.unwrap();
        let got: OutputMessage = rb.read().await.unwrap().expect("frame after blanks");
        assert_eq!(got, OutputMessage::Shutdown);
    }

    #[tokio::test]
    async fn disconnect_yields_clean_eof() {
        let (a, b) = tokio::io::duplex(1024);
        let (mut rb, _wb) = IpcStream::new(b).into_split();
        drop(a); // peer "crashes"
        let got = rb.read::<OutputMessage>().await.unwrap();
        assert!(got.is_none(), "EOF must surface as Ok(None)");
    }

    #[tokio::test]
    async fn malformed_frame_is_an_error_but_link_survives() {
        use tokio::io::AsyncWriteExt as _;
        let (mut a, b) = tokio::io::duplex(1024);
        let (mut rb, _wb) = IpcStream::new(b).into_split();
        a.write_all(b"{not json}\n").await.unwrap();
        assert!(rb.read::<OutputMessage>().await.is_err());
        // The next well-formed frame still parses.
        let json = serde_json::to_string(&OutputMessage::Heartbeat { at: 1 }).unwrap();
        a.write_all(json.as_bytes()).await.unwrap();
        a.write_all(b"\n").await.unwrap();
        let got: OutputMessage = rb.read().await.unwrap().expect("recovered");
        assert_eq!(got, OutputMessage::Heartbeat { at: 1 });
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_socket_listener_accepts_and_round_trips() {
        let path = endpoint_path(&format!("test-{}", std::process::id()));
        let mut listener = IpcListener::bind(&path).unwrap();
        let client = tokio::spawn({
            let path = path.clone();
            async move {
                let stream = connect(&path).await.unwrap();
                let (mut r, mut w) = stream.into_split();
                let msg: OutputMessage = r.read().await.unwrap().unwrap();
                let OutputMessage::Render { seq, .. } = msg else {
                    panic!("expected render");
                };
                w.write(&OutputAck::Rendered {
                    seq,
                    rendered_at: 9,
                })
                .await
                .unwrap();
            }
        });
        let stream = listener.accept().await.unwrap();
        let (mut r, mut w) = stream.into_split();
        w.write(&OutputMessage::Render {
            frame: LiveFrame::Logo,
            seq: 3,
        })
        .await
        .unwrap();
        let ack: OutputAck = r.read().await.unwrap().unwrap();
        assert_eq!(
            ack,
            OutputAck::Rendered {
                seq: 3,
                rendered_at: 9
            }
        );
        client.await.unwrap();
        drop(listener);
        assert!(!path.exists(), "socket file removed on drop");
    }

    #[test]
    fn endpoint_path_is_deterministic_and_sanitized() {
        let a = endpoint_path("output-main-0");
        let b = endpoint_path("output-main-0");
        assert_eq!(a, b);
        let weird = endpoint_path("output/../main 0");
        let s = weird.to_string_lossy();
        assert!(!s.contains(".."), "path traversal sanitized: {s}");
        assert!(!s.contains(' '));
    }
}
