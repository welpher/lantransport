use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

use lantransport_lib::server::ServerHandle;
use lantransport_lib::server::upload::UploadConfig;

struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

fn start(root: &Path, password: Option<&str>) -> ServerHandle {
    start_with_upload(root, password, UploadConfig::default())
}

fn start_with_upload(root: &Path, password: Option<&str>, upload: UploadConfig) -> ServerHandle {
    start_many(&[root], password, upload)
}

fn start_many(roots: &[&Path], password: Option<&str>, upload: UploadConfig) -> ServerHandle {
    let shares: Vec<std::path::PathBuf> = roots.iter().map(|root| root.to_path_buf()).collect();
    ServerHandle::start(
        &shares,
        0,
        false,
        password.map(str::to_string),
        upload,
        None,
    )
    .expect("server starts")
}

fn put(port: u16, query: &str, key: Option<&str>, body: &str) -> Reply {
    let auth = key.map_or(String::new(), |key| format!("X-Upload-Key: {key}\r\n"));
    send(
        port,
        &format!(
            "PUT /upload?{query} HTTP/1.1\r\nHost: test\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
    )
}

fn send(port: u16, raw: &str) -> Reply {
    // Parallel loopback tests reuse ports fast enough to hit TIME_WAIT resets now
    // and then; retrying idempotent requests keeps the suite deterministic.
    let attempts = if raw.starts_with("GET ") || raw.starts_with("HEAD ") {
        3
    } else {
        1
    };
    let mut last_error = None;
    for _ in 0..attempts {
        match try_send(port, raw) {
            Ok(reply) => return reply,
            Err(error) => last_error = Some(error),
        }
    }
    panic!(
        "request to 127.0.0.1:{port} failed: {}",
        last_error.expect("attempted")
    );
}

/// Send a raw request and return the full (possibly pipelined) response text.
fn raw_send(port: u16, raw: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    stream.write_all(raw.as_bytes()).expect("write");
    let mut reply = Vec::new();
    stream.read_to_end(&mut reply).expect("read");
    String::from_utf8_lossy(&reply).into_owned()
}

fn try_send(port: u16, raw: &str) -> std::io::Result<Reply> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(raw.as_bytes())?;
    let mut raw_reply = Vec::new();
    stream.read_to_end(&mut raw_reply)?;
    let split = raw_reply
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "no headers"))?;
    let head = String::from_utf8_lossy(&raw_reply[..split]).into_owned();
    let body = raw_reply[split + 4..].to_vec();
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "no status"))?;
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .collect();
    Ok(Reply {
        status,
        headers,
        body,
    })
}

fn get(port: u16, path: &str) -> Reply {
    send(
        port,
        &format!("GET {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n"),
    )
}

fn temp_root(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("lantransport-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("hello.txt"), b"hello").unwrap();
    std::fs::write(dir.join("a b.txt"), b"space").unwrap();
    std::fs::write(dir.join(".env"), b"secret").unwrap();
    std::fs::write(dir.join("sub/nested.bin"), b"0123456789").unwrap();
    std::fs::canonicalize(dir).unwrap()
}

#[test]
fn lists_visible_entries_and_hides_dotfiles() {
    let root = temp_root("listing");
    let server = start(&root, None);
    let reply = get(server.port, "/");
    assert_eq!(reply.status, 200);
    let page = reply.text();
    assert!(page.contains("hello.txt"));
    assert!(page.contains("sub/"));
    assert!(!page.contains(".env"));
    assert!(
        reply
            .header("content-security-policy")
            .unwrap()
            .contains("nonce-")
    );
}

#[test]
fn rejects_traversal_and_hidden_paths() {
    let root = temp_root("traversal");
    let server = start(&root, None);
    assert_eq!(get(server.port, "/../etc/passwd").status, 404);
    assert_eq!(get(server.port, "/sub/../../etc/passwd").status, 404);
    assert_eq!(get(server.port, "/.env").status, 404);
    assert_eq!(get(server.port, "/%2e%2e/etc/passwd").status, 404);
}

#[cfg(unix)]
#[test]
fn rejects_symlink_escape() {
    let root = temp_root("symlink");
    std::os::unix::fs::symlink("/etc", root.join("outside")).unwrap();
    let server = start(&root, None);
    assert_eq!(get(server.port, "/outside/passwd").status, 404);
}

#[test]
fn serves_percent_encoded_names() {
    let root = temp_root("encoded");
    let server = start(&root, None);
    let reply = get(server.port, "/a%20b.txt");
    assert_eq!(reply.status, 200);
    assert_eq!(reply.text(), "space");
}

#[test]
fn supports_range_requests() {
    let root = temp_root("range");
    let server = start(&root, None);
    let reply = send(
        server.port,
        "GET /sub/nested.bin HTTP/1.1\r\nHost: test\r\nRange: bytes=2-4\r\nConnection: close\r\n\r\n",
    );
    assert_eq!(reply.status, 206);
    assert_eq!(reply.header("content-range"), Some("bytes 2-4/10"));
    assert_eq!(reply.text(), "234");

    let reply = send(
        server.port,
        "GET /sub/nested.bin HTTP/1.1\r\nHost: test\r\nRange: bytes=50-60\r\nConnection: close\r\n\r\n",
    );
    assert_eq!(reply.status, 416);
    assert_eq!(reply.header("content-range"), Some("bytes */10"));
}

#[test]
fn head_returns_headers_without_body() {
    let root = temp_root("head");
    let server = start(&root, None);
    let reply = send(
        server.port,
        "HEAD /hello.txt HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n",
    );
    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("content-length"), Some("5"));
    assert!(reply.body.is_empty());
}

#[test]
fn download_forces_attachment() {
    let root = temp_root("download");
    let server = start(&root, None);
    let reply = get(server.port, "/hello.txt?download=1");
    assert_eq!(reply.status, 200);
    assert!(
        reply
            .header("content-disposition")
            .unwrap()
            .starts_with("attachment;")
    );
}

#[test]
fn password_auth_and_throttle() {
    let root = temp_root("auth");
    let server = start(&root, Some("s3cret"));
    assert_eq!(get(server.port, "/").status, 401);

    let authorized = send(
        server.port,
        &format!(
            "GET / HTTP/1.1\r\nHost: test\r\nAuthorization: Basic {}\r\nConnection: close\r\n\r\n",
            base64("share:s3cret")
        ),
    );
    assert_eq!(authorized.status, 200);

    for _ in 0..12 {
        get(server.port, "/");
    }
    assert_eq!(get(server.port, "/").status, 429);
}

fn base64(raw: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(raw)
}

#[test]
fn localizes_pages_per_visitor() {
    let root = temp_root("i18n");
    let server = start(&root, None);
    let zh = send(
        server.port,
        "GET / HTTP/1.1\r\nHost: test\r\nAccept-Language: zh-CN,zh;q=0.9\r\nConnection: close\r\n\r\n",
    );
    assert!(zh.text().contains("<html lang=\"zh-CN\">"));
    assert!(zh.text().contains("名称"));
    let en = get(server.port, "/?lang=en");
    assert!(en.text().contains("Name"));
    assert!(en.text().contains("<html lang=\"en\">"));
}

#[test]
fn rejects_non_get_methods() {
    let root = temp_root("method");
    let server = start(&root, None);
    let reply = send(
        server.port,
        "POST / HTTP/1.1\r\nHost: test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    assert_eq!(reply.status, 405);
    assert_eq!(reply.header("allow"), Some("GET, HEAD, PUT"));
}

fn upload_server(name: &str) -> (std::path::PathBuf, ServerHandle) {
    let root = temp_root(name);
    let server = start_with_upload(
        &root,
        None,
        UploadConfig {
            enabled: true,
            key: "k3y".into(),
            max_bytes: 1024,
        },
    );
    (root, server)
}

#[test]
fn uploads_with_valid_key() {
    let (root, server) = upload_server("upload");
    let reply = put(server.port, "name=note+one.txt", Some("k3y"), "payload");
    assert_eq!(reply.status, 201);
    assert_eq!(
        std::fs::read_to_string(root.join("uploads/note one.txt")).unwrap(),
        "payload"
    );

    let page = get(server.port, "/uploads/");
    assert!(page.text().contains("note one.txt"));
}

#[test]
fn upload_collision_gets_suffix() {
    let (root, server) = upload_server("collision");
    assert_eq!(
        put(server.port, "name=hello.txt", Some("k3y"), "new").status,
        201
    );
    assert_eq!(
        std::fs::read_to_string(root.join("uploads/hello.txt")).unwrap(),
        "new"
    );
    assert_eq!(
        put(server.port, "name=hello.txt", Some("k3y"), "second").status,
        201
    );
    assert_eq!(
        std::fs::read_to_string(root.join("uploads/hello (1).txt")).unwrap(),
        "second"
    );
}

#[test]
fn upload_rejects_bad_key_and_names() {
    let (root, server) = upload_server("upload-bad");
    assert_eq!(
        put(server.port, "name=x.txt", Some("nope"), "x").status,
        403
    );
    assert_eq!(put(server.port, "name=x.txt", None, "x").status, 403);
    assert_eq!(
        put(server.port, "name=..%2Fevil", Some("k3y"), "x").status,
        400
    );
    assert_eq!(
        put(server.port, "name=%2Ehidden", Some("k3y"), "x").status,
        400
    );
    let inbox = root.join("uploads");
    assert!(
        !inbox.exists()
            || std::fs::read_dir(&inbox).unwrap().all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".lantransport")
            })
    );
}

#[test]
fn upload_respects_size_limit_and_switch() {
    let (_, server) = upload_server("upload-size");
    let big = "a".repeat(2048);
    assert_eq!(
        put(server.port, "dir=%2F&name=big.bin", Some("k3y"), &big).status,
        413
    );

    let root = temp_root("upload-off");
    let off = start(&root, None);
    assert_eq!(
        put(off.port, "dir=%2F&name=x.txt", Some("k3y"), "x").status,
        403
    );
}

fn second_root(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("lantransport-second-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("second.txt"), b"two").unwrap();
    std::fs::write(dir.join("sub/other.txt"), b"other").unwrap();
    std::fs::canonicalize(dir).unwrap()
}

#[test]
fn merges_multiple_folders() {
    let first = temp_root("merge");
    let second = second_root("merge");
    let server = start_many(&[&first, &second], None, UploadConfig::default());

    let root = get(server.port, "/").text();
    assert!(root.contains("hello.txt"));
    assert!(root.contains("second.txt"));
    assert!(root.contains("sub/"));

    let sub = get(server.port, "/sub/").text();
    assert!(sub.contains("nested.bin"));
    assert!(sub.contains("other.txt"));
}

#[test]
fn duplicate_names_are_disambiguated_by_source() {
    let first = temp_root("dupe");
    let second = second_root("dupe");
    std::fs::write(first.join("notes.txt"), b"from-first").unwrap();
    std::fs::write(second.join("notes.txt"), b"from-second").unwrap();
    let server = start_many(&[&first, &second], None, UploadConfig::default());

    let page = get(server.port, "/").text();
    assert!(page.matches("notes.txt").count() >= 2);
    assert!(page.contains("src=0") && page.contains("src=1"));

    assert_eq!(get(server.port, "/notes.txt?src=0").text(), "from-first");
    assert_eq!(get(server.port, "/notes.txt?src=1").text(), "from-second");
    assert_eq!(get(server.port, "/notes.txt").text(), "from-first");
}

#[test]
fn uploads_land_in_the_first_folder_inbox() {
    let first = temp_root("inbox-first");
    let second = second_root("inbox-second");
    let server = start_many(
        &[&first, &second],
        None,
        UploadConfig {
            enabled: true,
            key: "k3y".into(),
            max_bytes: 1024,
        },
    );

    let reply = put(server.port, "name=dropped.txt", Some("k3y"), "payload");
    assert_eq!(reply.status, 201);
    assert_eq!(
        std::fs::read_to_string(first.join("uploads/dropped.txt")).unwrap(),
        "payload"
    );
    assert!(!second.join("uploads").exists());

    let inbox = get(server.port, "/uploads/").text();
    assert!(inbox.contains("dropped.txt"));
}

#[test]
fn accepts_chunked_uploads() {
    let (root, server) = upload_server("chunked");
    let raw = "PUT /upload?name=chunked.txt HTTP/1.1\r\nHost: test\r\nX-Upload-Key: k3y\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";
    let reply = send(server.port, raw);
    assert_eq!(reply.status, 201);
    assert_eq!(
        std::fs::read_to_string(root.join("uploads/chunked.txt")).unwrap(),
        "hello world"
    );
}

#[test]
fn rejects_oversized_chunked_uploads() {
    let (root, server) = upload_server("chunked-big");
    let chunk = "a".repeat(2000);
    let raw = format!(
        "PUT /upload?name=big.txt HTTP/1.1\r\nHost: test\r\nX-Upload-Key: k3y\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{chunk}\r\n0\r\n\r\n",
        chunk.len()
    );
    assert_eq!(send(server.port, &raw).status, 413);
    assert!(!root.join("uploads/big.txt").exists());
}

#[test]
fn serves_html_and_svg_as_forced_downloads() {
    let root = temp_root("active-content");
    std::fs::write(root.join("page.html"), b"<script>alert(1)</script>").unwrap();
    std::fs::write(
        root.join("pic.svg"),
        b"<svg xmlns='http://www.w3.org/2000/svg'/>",
    )
    .unwrap();
    let server = start(&root, None);

    for name in ["page.html", "pic.svg"] {
        let reply = get(server.port, &format!("/{name}"));
        assert_eq!(reply.status, 200, "{name}");
        assert!(
            reply
                .header("content-disposition")
                .is_some_and(|value| value.starts_with("attachment;")),
            "{name} should be an attachment"
        );
        assert_eq!(
            reply.header("content-security-policy"),
            Some("sandbox; default-src 'none'"),
            "{name} should be sandboxed"
        );
    }

    // Ordinary images stay inline so previews keep working.
    std::fs::write(root.join("ok.png"), b"png").unwrap();
    let reply = get(server.port, "/ok.png");
    assert_eq!(reply.status, 200);
    assert!(reply.header("content-disposition").is_none());
}

#[test]
fn parent_link_percent_encodes_names() {
    let root = temp_root("parent-link");
    std::fs::create_dir_all(root.join("a#b/deep")).unwrap();
    let server = start(&root, None);
    let page = get(server.port, "/a%23b/deep/").text();
    assert!(page.contains("data-href='/a%23b/'"), "got: {page}");
    assert!(!page.contains("data-href='/a#b/'"));
}

#[test]
fn keep_alive_survives_requests_with_bodies() {
    let root = temp_root("drain");
    let server = start(&root, None);
    // The 405 must not leave the POST body behind to be parsed as a request.
    let raw = "POST / HTTP/1.1\r\nHost: test\r\nContent-Length: 5\r\n\r\nhelloGET /hello.txt HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n";
    let text = raw_send(server.port, raw);
    assert!(text.contains("HTTP/1.1 405"), "got: {text}");
    assert!(
        text.contains("HTTP/1.1 200"),
        "second request dropped: {text}"
    );
    assert!(text.ends_with("hello"), "got: {text}");
}

#[test]
fn nested_share_roots_are_not_listed_twice() {
    let root = temp_root("nested");
    let nested = root.join("sub");
    let server = start_many(&[&root, &nested], None, UploadConfig::default());
    let page = get(server.port, "/").text();
    assert_eq!(
        page.matches("data-name='nested.bin'").count(),
        0,
        "nested contents must not be flattened into the parent listing"
    );
    let sub = get(server.port, "/sub/").text();
    assert_eq!(sub.matches("data-name='nested.bin'").count(), 1);
}

#[test]
fn password_change_applies_to_a_running_server() {
    let root = temp_root("password-change");
    let server = start(&root, Some("old-pass"));
    assert_eq!(
        send(
            server.port,
            &format!(
                "GET / HTTP/1.1\r\nHost: test\r\nAuthorization: Basic {}\r\nConnection: close\r\n\r\n",
                base64("share:old-pass")
            ),
        )
        .status,
        200
    );

    server.set_password(Some("new-pass".into()));
    assert_eq!(
        send(
            server.port,
            &format!(
                "GET / HTTP/1.1\r\nHost: test\r\nAuthorization: Basic {}\r\nConnection: close\r\n\r\n",
                base64("share:old-pass")
            ),
        )
        .status,
        401
    );
    assert_eq!(
        send(
            server.port,
            &format!(
                "GET / HTTP/1.1\r\nHost: test\r\nAuthorization: Basic {}\r\nConnection: close\r\n\r\n",
                base64("share:new-pass")
            ),
        )
        .status,
        200
    );
}
