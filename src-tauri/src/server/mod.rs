//! The embedded read-only HTTP server.

pub mod auth;
pub mod http;
pub mod i18n;
pub mod lan;
pub mod listing;
pub mod paths;
pub mod route;
pub mod upload;

use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, Ipv4Addr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
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

pub struct ServerContext {
    pub log: Option<PathBuf>,
    pub mounts: Vec<Mount>,
    pub auth: RwLock<auth::Auth>,
    pub limiter: auth::AuthLimiter,
    pub upload: RwLock<UploadConfig>,
    pub upload_limiter: auth::AuthLimiter,
}

pub struct ServerHandle {
    pub port: u16,
    pub urls: Vec<String>,
    pub started_at: SystemTime,
    shutdown: Arc<AtomicBool>,
    context: Arc<ServerContext>,
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

impl ServerHandle {
    pub fn start(
        shares: &[PathBuf],
        port: u16,
        allow_lan: bool,
        password: Option<String>,
        upload: UploadConfig,
        log: Option<PathBuf>,
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

        let host = if allow_lan {
            Ipv4Addr::UNSPECIFIED
        } else {
            Ipv4Addr::LOCALHOST
        };
        let listener = TcpListener::bind((host, port)).map_err(|error| match error.kind() {
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

        let context = Arc::new(ServerContext {
            log,
            mounts,
            auth: RwLock::new(auth::Auth::new(password)),
            limiter: auth::AuthLimiter::default(),
            upload: RwLock::new(upload),
            upload_limiter: auth::AuthLimiter::default(),
        });
        let context_for_handle = Arc::clone(&context);
        let shutdown = Arc::new(AtomicBool::new(false));
        let active = Arc::new(AtomicUsize::new(0));
        let stop_flag = Arc::clone(&shutdown);
        let thread = thread::Builder::new()
            .name("lantransport-http".into())
            .spawn(move || accept_loop(listener, context, stop_flag, active))
            .map_err(|error| StartError::Io(error.to_string()))?;

        Ok(Self {
            port,
            urls: lan::urls(port, allow_lan),
            started_at: SystemTime::now(),
            shutdown,
            context: Arc::clone(&context_for_handle),
            thread: Some(thread),
        })
    }

    pub fn stop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
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

fn handle_connection(mut stream: TcpStream, context: Arc<ServerContext>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(15)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(60)));
    let _ = stream.set_nodelay(true);
    let peer: IpAddr = stream
        .peer_addr()
        .map_or(IpAddr::from(Ipv4Addr::UNSPECIFIED), |addr| addr.ip());

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
                    let mut response = if http::split_target(&request.target).0 == "/upload" {
                        upload::handle(&mut reader, &request, &context, peer)
                    } else {
                        http::Response::empty(405).header("Allow", "GET, HEAD, PUT")
                    };
                    let _ = http::write_response(&mut stream, false, false, &mut response);
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

/// Reused across events so the hot auth-failure path does not re-open the file.
static LOG_FILE: OnceLock<Mutex<Option<(PathBuf, std::fs::File)>>> = OnceLock::new();

pub fn log_event(ctx: &ServerContext, message: &str) {
    let line = format!(
        "{} {}
",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        message
    );
    if let Some(path) = &ctx.log {
        let slot = LOG_FILE.get_or_init(|| Mutex::new(None));
        let mut guard = slot.lock().unwrap_or_else(|error| error.into_inner());
        if guard.as_ref().is_none_or(|(current, _)| current != path) {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            *guard = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .ok()
                .map(|file| (path.clone(), file));
        }
        if let Some((_, file)) = guard.as_mut() {
            let _ = file.write_all(line.as_bytes());
        }
    }
    eprint!("{line}");
}
