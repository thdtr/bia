//! A connection to one language server: JSON-RPC messages framed with a
//! `Content-Length` header over the server's stdin and stdout.
//!
//! A thread reads what the server sends and hands it to a callback as an
//! [`Event`]. Requests the server makes of BIA are answered right there.

use super::text::Encoding;
use serde_json::{Value, json};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub type ServerId = usize;

/// What a server sent.
#[derive(Debug, Clone)]
pub enum Event {
    /// The server answered `initialize`: it's ready for documents.
    Ready {
        server: ServerId,
        encoding: Encoding,
        capabilities: Value,
    },
    Response {
        server: ServerId,
        id: u64,
        result: Result<Value, String>,
    },
    Notification {
        server: ServerId,
        method: String,
        params: Value,
    },
    /// The server stopped, or never started properly.
    Exited {
        server: ServerId,
        message: String,
    },
}

type Writer = Arc<Mutex<Box<dyn Write + Send>>>;

pub struct Client {
    pub id: ServerId,
    writer: Writer,
    next_id: AtomicU64,
    child: Option<Child>,
}

/// The id of the `initialize` request.
const INITIALIZE: u64 = 0;

impl Client {
    /// Starts `command` (program and arguments) in `root` and connects.
    ///
    /// Servers are separate programs, but some reach for the network on
    /// their own (rust-analyzer runs cargo, gopls runs go). Their
    /// environment turns that off where a variable for it exists.
    pub fn spawn(
        id: ServerId,
        command: &[String],
        root: &Path,
        sink: impl Fn(Event) + Send + 'static,
    ) -> io::Result<Self> {
        let (program, args) =
            command.split_first().ok_or_else(|| io::Error::other("empty command"))?;
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(root)
            .env("CARGO_NET_OFFLINE", "true")
            .env("GOPROXY", "off")
            .env("GOTOOLCHAIN", "local")
            .env("PIP_NO_INDEX", "1")
            .env("NPM_CONFIG_OFFLINE", "true")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| io::Error::other("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| io::Error::other("no stdout"))?;
        let mut client = Self::connect(id, stdout, stdin, root, sink);
        client.child = Some(child);
        Ok(client)
    }

    /// Talks to a server over `reader` and `writer`, and sends `initialize`.
    pub fn connect(
        id: ServerId,
        reader: impl Read + Send + 'static,
        writer: impl Write + Send + 'static,
        root: &Path,
        sink: impl Fn(Event) + Send + 'static,
    ) -> Self {
        let writer: Writer = Arc::new(Mutex::new(Box::new(writer)));
        let client =
            Self { id, writer: Arc::clone(&writer), next_id: AtomicU64::new(1), child: None };
        let root_uri = super::text::uri(root);
        client.send(&json!({
            "jsonrpc": "2.0",
            "id": INITIALIZE,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "clientInfo": { "name": "BIA" },
                "rootUri": root_uri,
                "workspaceFolders": [{ "uri": root_uri, "name": root.file_name().map(|n| n.to_string_lossy()).unwrap_or_default() }],
                "capabilities": capabilities(),
            }
        }));
        std::thread::spawn(move || read_loop(id, BufReader::new(reader), &writer, &sink));
        client
    }

    fn send(&self, message: &Value) {
        if let Ok(mut w) = self.writer.lock() {
            // A server that went away shows up as `Exited` from the reader.
            let _ = write_message(&mut *w, message);
        }
    }

    /// Sends a request; its answer comes back as an [`Event::Response`]
    /// with the returned id.
    pub fn request(&self, method: &str, params: Value) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        id
    }

    pub fn notify(&self, method: &str, params: Value) {
        self.send(&json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }
}

impl Drop for Client {
    /// Asks the server to stop, then makes sure it does.
    fn drop(&mut self) {
        self.request("shutdown", Value::Null);
        self.notify("exit", Value::Null);
        if let Some(mut child) = self.child.take() {
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let _ = child.kill();
                let _ = child.wait();
            });
        }
    }
}

fn capabilities() -> Value {
    json!({
        "general": { "positionEncodings": ["utf-8", "utf-16"] },
        "textDocument": {
            "synchronization": { "didSave": true },
            "publishDiagnostics": {},
            "hover": { "contentFormat": ["plaintext", "markdown"] },
            "definition": {},
            "completion": { "completionItem": { "snippetSupport": false } },
            "formatting": {},
            "rename": {}
        },
        "workspace": { "workspaceFolders": true },
        "window": {}
    })
}

pub fn write_message(w: &mut dyn Write, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message).map_err(io::Error::other)?;
    write!(w, "Content-Length: {}\r\n\r\n", body.len())?;
    w.write_all(&body)?;
    w.flush()
}

/// The next message, or `None` at the end of the stream. Lines before the
/// header that aren't headers are skipped: some servers print to stdout.
pub fn read_message(r: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            if length.is_some() {
                break;
            }
            continue;
        }
        if let Some(n) = line.strip_prefix("Content-Length:") {
            length = n.trim().parse::<usize>().ok();
        }
    }
    let mut body = vec![0; length.unwrap_or(0)];
    r.read_exact(&mut body)?;
    serde_json::from_slice(&body).map(Some).map_err(io::Error::other)
}

fn read_loop(id: ServerId, mut reader: impl BufRead, writer: &Writer, sink: &dyn Fn(Event)) {
    let reply = |message: Value| {
        if let Ok(mut w) = writer.lock() {
            let _ = write_message(&mut **w, &message);
        }
    };
    loop {
        let message = match read_message(&mut reader) {
            Ok(Some(m)) => m,
            Ok(None) => {
                sink(Event::Exited { server: id, message: "the server stopped".into() });
                return;
            }
            Err(e) => {
                sink(Event::Exited { server: id, message: e.to_string() });
                return;
            }
        };
        let method = message.get("method").and_then(Value::as_str).map(str::to_owned);
        let msg_id = message.get("id").cloned();
        match (method, msg_id) {
            // A request from the server.
            (Some(method), Some(msg_id)) => {
                let result = match method.as_str() {
                    "workspace/configuration" => {
                        let n = message["params"]["items"].as_array().map_or(0, Vec::len);
                        json!(vec![Value::Null; n])
                    }
                    "workspace/applyEdit" => json!({ "applied": false }),
                    "window/workDoneProgress/create"
                    | "client/registerCapability"
                    | "client/unregisterCapability"
                    | "window/showMessageRequest" => Value::Null,
                    _ => {
                        reply(
                            json!({ "jsonrpc": "2.0", "id": msg_id, "error": { "code": -32601, "message": "not supported" } }),
                        );
                        continue;
                    }
                };
                reply(json!({ "jsonrpc": "2.0", "id": msg_id, "result": result }));
            }
            (Some(method), None) => {
                let params = message.get("params").cloned().unwrap_or(Value::Null);
                sink(Event::Notification { server: id, method, params });
            }
            (None, Some(msg_id)) => {
                let Some(n) = msg_id.as_u64() else { continue };
                let result = match message.get("error") {
                    Some(e) => Err(e["message"].as_str().unwrap_or("error").to_owned()),
                    None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
                };
                if n == INITIALIZE {
                    let Ok(result) = result else {
                        sink(Event::Exited { server: id, message: "initialize failed".into() });
                        return;
                    };
                    reply(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
                    let capabilities = result.get("capabilities").cloned().unwrap_or(Value::Null);
                    let encoding = match capabilities["positionEncoding"].as_str() {
                        Some("utf-8") => Encoding::Utf8,
                        _ => Encoding::Utf16,
                    };
                    sink(Event::Ready { server: id, encoding, capabilities });
                } else {
                    sink(Event::Response { server: id, id: n, result });
                }
            }
            (None, None) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    /// A server in a thread, connected by pipes: answers `initialize`
    /// (asking for UTF-8), echoes `test/echo`, asks the client for its
    /// configuration, and publishes a diagnostic.
    fn fake_server() -> (std::io::PipeReader, std::io::PipeWriter) {
        let (client_reads, mut server_writes) = std::io::pipe().unwrap();
        let (server_reads, client_writes) = std::io::pipe().unwrap();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(server_reads);
            while let Ok(Some(m)) = read_message(&mut reader) {
                let id = m.get("id").cloned();
                match m["method"].as_str() {
                    Some("initialize") => {
                        let result = json!({ "capabilities": { "positionEncoding": "utf-8" } });
                        write_message(
                            &mut server_writes,
                            &json!({ "jsonrpc": "2.0", "id": id, "result": result }),
                        )
                        .unwrap();
                    }
                    Some("initialized") => {
                        // Noise some servers print, then a request of its own.
                        server_writes.write_all(b"booting...\n").unwrap();
                        let ask = json!({ "jsonrpc": "2.0", "id": 99, "method": "workspace/configuration", "params": { "items": [{}, {}] } });
                        write_message(&mut server_writes, &ask).unwrap();
                    }
                    Some("test/echo") => {
                        write_message(
                            &mut server_writes,
                            &json!({ "jsonrpc": "2.0", "id": id, "result": m["params"] }),
                        )
                        .unwrap();
                        let diag = json!({ "jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": { "uri": "file:///a", "diagnostics": [] } });
                        write_message(&mut server_writes, &diag).unwrap();
                    }
                    None if id == Some(json!(99)) => {
                        assert_eq!(m["result"], json!([null, null]));
                    }
                    _ => {}
                }
            }
        });
        (client_reads, client_writes)
    }

    #[test]
    fn handshake_requests_and_notifications() {
        let (reader, writer) = fake_server();
        let (tx, rx) = mpsc::channel();
        let client = Client::connect(7, reader, writer, Path::new("/tmp"), move |e| {
            let _ = tx.send(e);
        });
        let next = || rx.recv_timeout(Duration::from_secs(5)).expect("an event");
        match next() {
            Event::Ready { server: 7, encoding: Encoding::Utf8, .. } => {}
            other => panic!("{other:?}"),
        }
        let id = client.request("test/echo", json!({ "x": 1 }));
        match next() {
            Event::Response { id: got, result: Ok(v), .. } => {
                assert_eq!(got, id);
                assert_eq!(v, json!({ "x": 1 }));
            }
            other => panic!("{other:?}"),
        }
        match next() {
            Event::Notification { method, .. } => {
                assert_eq!(method, "textDocument/publishDiagnostics")
            }
            other => panic!("{other:?}"),
        }
    }
}
