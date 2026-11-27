//! The embedded read-only HTTP server.

pub mod auth;
pub mod http;
pub mod i18n;
pub mod lan;
pub mod listing;
pub mod paths;
pub mod route;
pub mod upload;
pub mod zip;

use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use crate::server::http::{Method, ReadOutcome};
use crate::server::upload::UploadConfig;

const MAX_CONNECTIONS: usize = 64;
const MAX_REQUESTS_PER_CONNECTION: usize = 100;

#[derive(Clone)]
pub struct Mount {
    pub label: String,
    pub dir: PathBuf,
}

/// What a finished upload looks like to the desktop app.
pub struct UploadNotice {
    pub name: String,
    pub bytes: u64,
    pub peer: std::net::IpAddr,
    pub dir: PathBuf,
    /// Set when the page uploads several files in one go, so the app can show
    /// one entry and one notification for the whole drop.
    pub batch: Option<String>,
}

pub type UploadHook = Arc<dyn Fn(UploadNotice) + Send + Sync>;

/// The page announces a drop and later says it is finished, so the app can
/// start a row as soon as the first file lands and finish it at the end.
pub struct UploadBatch {
    pub id: String,
    pub label: Option<String>,
    pub peer: std::net::IpAddr,
    /// Files and bytes the page is about to send, when it knows.
    pub count: u32,
    pub total: u64,
    pub done: bool,
}

pub type BatchHook = Arc<dyn Fn(UploadBatch) + Send + Sync>;

/// Callbacks the desktop app hangs off the server; the server itself stays
/// Tauri-free.
#[derive(Clone, Default)]
pub struct Hooks {
    pub on_urls_changed: Option<Arc<dyn Fn() + Send + Sync>>,
    pub on_upload: Option<UploadHook>,
    pub on_batch_end: Option<BatchHook>,
}

pub struct ServerContext {
    pub log: Option<PathBuf>,
    pub mounts: Vec<Mount>,
    pub auth: RwLock<auth::Auth>,
    pub limiter: auth::AuthLimiter,
    pub upload: RwLock<UploadConfig>,
    pub upload_limiter: auth::AuthLimiter,
    pub on_upload: Option<UploadHook>,
    pub on_batch_end: Option<BatchHook>,
}

pub struct ServerHandle {
    pub port: u16,
    pub urls: Arc<RwLock<Vec<String>>>,
    pub started_at: SystemTime,
    shutdown: Arc<AtomicBool>,
    context: Arc<ServerContext>,
    urls_tx: Option<Sender<()>>,
    monitor: Option<JoinHandle<()>>,
    thread: Option<JoinHandle<()>>,
}

#[derive(Debug)]
pub enum StartError {
    InvalidRoot,
    PortInUse,
    PermissionDenied,
    Io(String),
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartError::InvalidRoot => write!(f, "folder does not exist or is not a folder"),
            StartError::PortInUse => write!(f, "port is already in use"),
            StartError::PermissionDenied => write!(f, "permission denied"),
            StartError::Io(message) => write!(f, "{message}"),
        }
    }
}

/// Dual-stack when sharing on the LAN: IPv4 and IPv6 phones both connect, which
/// matters on IPv6-only hotspots. Falls back to IPv4 if the host has no IPv6.
///
/// Windows defaults an IPv6 socket to `IPV6_V6ONLY=1`, so binding `[::]` there
/// accepts *only* IPv6 and every IPv4 link breaks. socket2 lets us turn that off
/// explicitly instead of relying on the platform default.
fn bind(port: u16, allow_lan: bool) -> std::io::Result<TcpListener> {
    if allow_lan {
        match bind_dual_stack(port) {
            Ok(listener) => return Ok(listener),
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => return Err(error),
            // No IPv6 on this host: fall through to IPv4 only.
            Err(_) => {}
        }
    }
    let host = if allow_lan {
        Ipv4Addr::UNSPECIFIED
    } else {
        Ipv4Addr::LOCALHOST
    };
    TcpListener::bind((host, port))
}

fn bind_dual_stack(port: u16) -> std::io::Result<TcpListener> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket = Socket::new(Domain::IPV6, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_only_v6(false)?;
    // A finished upload closes from this side, so the port keeps TIME_WAIT
    // entries; without this, starting again right after a stop says "port in
    // use" for up to a minute. std's TcpListener sets it on Unix too; the
    // socket2 path has to ask for it.
    socket.set_reuse_address(true)?;
    socket.bind(&std::net::SocketAddr::from((Ipv6Addr::UNSPECIFIED, port)).into())?;
    socket.listen(1024)?;
    Ok(socket.into())
}

/// Dual-stack sockets hand IPv4 clients over as `::ffff:a.b.c.d`; everything the
/// user sees (logs, notifications, throttling) should say `a.b.c.d`.
fn unmap_peer(peer: std::net::IpAddr) -> std::net::IpAddr {
    match peer {
        std::net::IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(std::net::IpAddr::V4)
            .unwrap_or(peer),
        other => other,
    }
}

impl ServerHandle {
    pub fn start(
        shares: &[PathBuf],
        port: u16,
        allow_lan: bool,
        password: Option<String>,
        upload: UploadConfig,
        log: Option<PathBuf>,
        hooks: Option<Hooks>,
    ) -> Result<Self, StartError> {
        let mut mounts = Vec::new();
        for share in shares {
            let dir = std::fs::canonicalize(share).map_err(|_| StartError::InvalidRoot)?;
            if !dir.is_dir() {
                return Err(StartError::InvalidRoot);
            }
            if mounts.iter().any(|mount: &Mount| mount.dir == dir) {
                continue;
            }
            let label = dir
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| dir.display().to_string());
            mounts.push(Mount { label, dir });
        }
        if mounts.is_empty() {
            return Err(StartError::InvalidRoot);
        }
        // A mount nested inside another one is already reachable through its
        // parent; keeping it would list its contents twice at every level.
        let roots: Vec<PathBuf> = mounts.iter().map(|mount| mount.dir.clone()).collect();
        mounts.retain(|mount| {
            !roots
                .iter()
                .any(|root| root != &mount.dir && mount.dir.starts_with(root))
        });
        let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for mount in &mut mounts {
            let count = seen.entry(mount.label.clone()).or_insert(0);
            *count += 1;
            if *count > 1 {
                mount.label = format!("{} ({count})", mount.label);
            }
        }

        let listener = bind(port, allow_lan).map_err(|error| match error.kind() {
            std::io::ErrorKind::AddrInUse => StartError::PortInUse,
            std::io::ErrorKind::PermissionDenied => StartError::PermissionDenied,
            _ => StartError::Io(error.to_string()),
        })?;
        let port = listener
            .local_addr()
            .map_err(|error| StartError::Io(error.to_string()))?
            .port();
        listener
            .set_nonblocking(true)
            .map_err(|error| StartError::Io(error.to_string()))?;

        let Hooks {
            on_urls_changed,
            on_upload,
            on_batch_end,
        } = hooks.unwrap_or_default();
        let context = Arc::new(ServerContext {
            log,
            mounts,
            auth: RwLock::new(auth::Auth::new(password)),
            limiter: auth::AuthLimiter::default(),
            upload: RwLock::new(upload),
            upload_limiter: auth::AuthLimiter::default(),
            on_upload,
            on_batch_end,
        });
        let context_for_handle = Arc::clone(&context);
        let shutdown = Arc::new(AtomicBool::new(false));
        let active = Arc::new(AtomicUsize::new(0));
        let stop_flag = Arc::clone(&shutdown);
        let thread = thread::Builder::new()
            .name("lantransport-http".into())
            .spawn(move || accept_loop(listener, context, stop_flag, active))
            .map_err(|error| StartError::Io(error.to_string()))?;

        // Addresses change when the machine switches networks; keep the
        // advertised URLs in sync without restarting the listener.
        let urls = Arc::new(RwLock::new(lan::urls(port, allow_lan)));
        let (urls_tx, urls_rx) = mpsc::channel::<()>();
        let monitor_urls = Arc::clone(&urls);
        let monitor = thread::Builder::new()
            .name("lantransport-net".into())
            .spawn(move || {
                loop {
                    if urls_rx.recv_timeout(Duration::from_secs(15)).is_ok() {
                        break;
                    }
                    let next = lan::urls(port, allow_lan);
                    let changed = {
                        let mut current = monitor_urls
                            .write()
                            .unwrap_or_else(|error| error.into_inner());
                        if *current == next {
                            false
                        } else {
                            *current = next;
                            true
                        }
                    };
                    if changed && let Some(notify) = &on_urls_changed {
                        notify();
                    }
                }
            })
            .map_err(|error| StartError::Io(error.to_string()))?;

        Ok(Self {
            port,
            urls,
            started_at: SystemTime::now(),
            shutdown,
            context: Arc::clone(&context_for_handle),
            urls_tx: Some(urls_tx),
            monitor: Some(monitor),
            thread: Some(thread),
        })
    }

    pub fn stop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(sender) = self.urls_tx.take() {
            let _ = sender.send(());
        }
        if let Some(monitor) = self.monitor.take() {
            let _ = monitor.join();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    pub fn mount_labels(&self) -> Vec<String> {
        self.context
            .mounts
            .iter()
            .map(|mount| mount.label.clone())
            .collect()
    }

    pub fn set_upload(&self, config: UploadConfig) {
        *self
            .context
            .upload
            .write()
            .unwrap_or_else(|error| error.into_inner()) = config;
    }

    pub fn set_password(&self, password: Option<String>) {
        *self
            .context
            .auth
            .write()
            .unwrap_or_else(|error| error.into_inner()) = auth::Auth::new(password);
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

fn accept_loop(
    listener: TcpListener,
    context: Arc<ServerContext>,
    shutdown: Arc<AtomicBool>,
    active: Arc<AtomicUsize>,
) {
    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                // BSD/macOS accept() inherits O_NONBLOCK from the listener, which
                // turns body reads into EAGAIN instead of blocking.
                let _ = stream.set_nonblocking(false);
                if active.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
                    let mut stream = stream;
                    let _ = http::status_line_only(&mut stream, 503);
                    continue;
                }
                active.fetch_add(1, Ordering::Relaxed);
                let context = Arc::clone(&context);
                let active_guard = ActiveGuard(Arc::clone(&active));
                let _ = thread::Builder::new()
                    .name("lantransport-conn".into())
                    .spawn(move || {
                        let _guard = active_guard;
                        handle_connection(stream, context);
                    });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(_) => thread::sleep(Duration::from_millis(100)),
        }
    }
}

struct ActiveGuard(Arc<AtomicUsize>);

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Consume any request body left on a kept-alive connection. Returns false
/// when the body cannot be skipped (oversized/undelimited), so the caller
/// closes the connection instead of parsing the leftover bytes as a request.
fn discard_body<R: BufRead>(reader: &mut R, request: &http::Request) -> bool {
    const MAX_DISCARD: u64 = 1024 * 1024;
    let chunked = request
        .header("transfer-encoding")
        .is_some_and(|value| value.to_ascii_lowercase().contains("chunked"));
    if chunked {
        // Do not attempt to parse chunk framing here; a stray chunked body is
        // not worth the parser, just close the connection.
        return false;
    }
    let Some(length) = request
        .header("content-length")
        .and_then(|value| value.parse::<u64>().ok())
    else {
        return true;
    };
    if length == 0 {
        return true;
    }
    if length > MAX_DISCARD {
        return false;
    }
    let mut limited = std::io::Read::take(&mut *reader, length);
    std::io::copy(&mut limited, &mut std::io::sink()).is_ok()
}

/// Close a connection that still has unread request bytes in flight.
///
/// Closing such a socket makes the kernel send RST, which can discard a
/// response the client has not finished reading. Shutting down the write half
/// first sends FIN, so the client sees a clean end of stream after the
/// response; the brief read that follows lets the peer finish sending before
/// the socket is dropped. Bounded, and it never blocks for long: a client that
/// has already stopped sending just sees the read timeout.
fn close_with_unread_body<R: BufRead>(reader: &mut R, stream: &TcpStream) {
    let _ = stream.shutdown(std::net::Shutdown::Write);
    let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
    let _ = std::io::copy(
        &mut std::io::Read::take(&mut *reader, 1024 * 1024),
        &mut std::io::sink(),
    );
}

fn handle_connection(mut stream: TcpStream, context: Arc<ServerContext>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(15)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(60)));
    let _ = stream.set_nodelay(true);
    let peer: IpAddr = stream
        .peer_addr()
        .map_or(IpAddr::from(Ipv4Addr::UNSPECIFIED), |addr| {
            unmap_peer(addr.ip())
        });

    let Ok(read_stream) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(read_stream);

    for _ in 0..MAX_REQUESTS_PER_CONNECTION {
        match http::read_request(&mut reader) {
            Ok(ReadOutcome::Eof) => break,
            Ok(ReadOutcome::Malformed(status)) => {
                let _ = http::status_line_only(&mut stream, status);
                break;
            }
            Ok(ReadOutcome::Request(request)) => {
                if request.method == Method::Put {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(300)));
                    if request
                        .header("expect")
                        .is_some_and(|value| value.eq_ignore_ascii_case("100-continue"))
                    {
                        let _ = stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n");
                        let _ = stream.flush();
                    }
                    let (mut response, body_read) =
                        if http::split_target(&request.target).0 == "/upload" {
                            upload::handle(&mut reader, &request, &context, peer)
                        } else {
                            (
                                http::Response::empty(405).header("Allow", "GET, HEAD, PUT"),
                                false,
                            )
                        };
                    let _ = http::write_response(&mut stream, false, false, &mut response);
                    // A rejected upload leaves the client still streaming its
                    // body. Closing on that unread data sends RST, which can
                    // discard the response before the client reads it.
                    if !body_read {
                        close_with_unread_body(&mut reader, &stream);
                    }
                    break;
                }
                let keep_alive = request.wants_keep_alive();
                let head_only = request.method == Method::Head;
                let mut response = route::handle(&request, &context, peer);
                if http::write_response(&mut stream, keep_alive, head_only, &mut response).is_err()
                {
                    return;
                }
                if !keep_alive {
                    break;
                }
                if !discard_body(&mut reader, &request) {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

/// One log file is capped at this size; when it fills, it is renamed to `<path>.1`
/// (replacing the previous `.1`) and a fresh file is started. Two files, 20 MB
/// worst case: enough to keep the run that crashed without growing forever.
pub const LOG_LIMIT: u64 = 10 * 1024 * 1024;

pub fn log_event(ctx: &ServerContext, message: &str) {
    let line = format!(
        "{} {}
",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        message
    );
    if let Some(path) = &ctx.log {
        append_line(path, &line, LOG_LIMIT);
    }
    eprint!("{line}");
}

/// Appends one already-formatted line, rotating first when the file is full.
/// Every writer goes through here: holding a cached handle while another writer
/// rotates would leave it writing into the renamed inode.
pub fn append_line(path: &std::path::Path, line: &str, limit: u64) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(path)
        .map(|meta| meta.len() >= limit)
        .unwrap_or(false)
    {
        let previous = path.with_extension("log.1");
        let _ = std::fs::remove_file(&previous);
        let _ = std::fs::rename(path, &previous);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = file.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod log_tests {
    use super::*;

    #[test]
    fn rotates_once_the_file_reaches_the_limit() {
        let dir = std::env::temp_dir().join(format!("lt-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lantransport.log");

        append_line(&path, "first\n", 32);
        append_line(&path, "second line that fills it\n", 32);
        assert!(std::fs::metadata(&path).unwrap().len() >= 32);

        append_line(&path, "after rotation\n", 32);
        let current = std::fs::read_to_string(&path).unwrap();
        let previous = std::fs::read_to_string(dir.join("lantransport.log.1")).unwrap();
        assert_eq!(current, "after rotation\n");
        assert!(previous.contains("second line"), "older bytes moved aside");

        // a second rotation replaces the previous file instead of piling up
        append_line(&path, "more than thirty two bytes of text here\n", 32);
        assert!(dir.join("lantransport.log.1").exists());
        let entries = std::fs::read_dir(&dir).unwrap().count();
        assert_eq!(entries, 2, "only the current file and one rotation");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
