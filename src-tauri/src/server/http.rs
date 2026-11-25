//! Minimal HTTP/1.1 request parsing and response writing for a read-only file server.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, Read, Seek, SeekFrom, Write};
use std::net::TcpStream;

use chrono::Utc;

const MAX_LINE: usize = 8 * 1024;
const MAX_HEADERS_BYTES: usize = 16 * 1024;
const MAX_HEADERS: usize = 64;
pub const CHUNK_SIZE: usize = 256 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Head,
    Put,
    Other(String),
}

impl Method {
    fn parse(raw: &str) -> Self {
        match raw {
            "GET" => Method::Get,
            "HEAD" => Method::Head,
            "PUT" => Method::Put,
            other => Method::Other(other.to_string()),
        }
    }
}

#[derive(Debug)]
pub struct Request {
    pub method: Method,
    pub target: String,
    pub http11: bool,
    pub headers: HashMap<String, String>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }

    pub fn wants_keep_alive(&self) -> bool {
        match self.header("connection") {
            Some(value) if value.eq_ignore_ascii_case("close") => false,
            Some(value) if value.eq_ignore_ascii_case("keep-alive") => true,
            _ => self.http11,
        }
    }
}

pub enum ReadOutcome {
    Request(Request),
    Eof,
    Malformed(u16),
}

pub fn read_request<R: BufRead>(reader: &mut R) -> io::Result<ReadOutcome> {
    let mut budget = MAX_HEADERS_BYTES;
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = read_line(reader, &mut line, MAX_LINE)?;
        if read == 0 {
            return Ok(ReadOutcome::Eof);
        }
        budget = budget.saturating_sub(read);
        if !line.is_empty() {
            break;
        }
        if budget == 0 {
            return Ok(ReadOutcome::Malformed(431));
        }
    }

    let request_line = String::from_utf8_lossy(&line).trim_end().to_string();
    let mut parts = request_line.split_whitespace();
    let (Some(method), Some(target), Some(version)) = (parts.next(), parts.next(), parts.next())
    else {
        return Ok(ReadOutcome::Malformed(400));
    };
    if parts.next().is_some() {
        return Ok(ReadOutcome::Malformed(400));
    }
    let http11 = match version {
        "HTTP/1.1" => true,
        "HTTP/1.0" => false,
        _ => return Ok(ReadOutcome::Malformed(505)),
    };
    if target.len() > MAX_LINE || target.contains('\0') {
        return Ok(ReadOutcome::Malformed(414));
    }

    let mut headers = HashMap::new();
    let mut count = 0;
    loop {
        line.clear();
        let read = read_line(reader, &mut line, MAX_LINE)?;
        if read == 0 {
            return Ok(ReadOutcome::Eof);
        }
        budget = budget.saturating_sub(read);
        if budget == 0 {
            return Ok(ReadOutcome::Malformed(431));
        }
        let text = String::from_utf8_lossy(&line);
        let text = text.trim_end_matches(['\r', '\n']);
        if text.is_empty() {
            break;
        }
        count += 1;
        if count > MAX_HEADERS {
            return Ok(ReadOutcome::Malformed(431));
        }
        let Some((name, value)) = text.split_once(':') else {
            return Ok(ReadOutcome::Malformed(400));
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        if name.is_empty() || name.bytes().any(|b| b <= b' ' || b >= 0x7f) {
            return Ok(ReadOutcome::Malformed(400));
        }
        headers
            .entry(name)
            .and_modify(|existing: &mut String| {
                existing.push_str(", ");
                existing.push_str(value);
            })
            .or_insert_with(|| value.to_string());
    }

    Ok(ReadOutcome::Request(Request {
        method: Method::parse(method),
        target: target.to_string(),
        http11,
        headers,
    }))
}

fn read_line<R: BufRead>(reader: &mut R, out: &mut Vec<u8>, max: usize) -> io::Result<usize> {
    let read = reader.take(max as u64 + 1).read_until(b'\n', out)?;
    if out.len() > max {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "line too long"));
    }
    Ok(read)
}

pub enum Body {
    Empty,
    Bytes(Vec<u8>),
    File { file: File, offset: u64, len: u64 },
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Body,
    pub content_length: u64,
}

impl Response {
    pub fn empty(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Body::Empty,
            content_length: 0,
        }
    }

    pub fn bytes(status: u16, content_type: &str, body: Vec<u8>) -> Self {
        let len = body.len() as u64;
        Self {
            status,
            headers: vec![("Content-Type".into(), content_type.into())],
            body: Body::Bytes(body),
            content_length: len,
        }
    }

    pub fn html(status: u16, body: String) -> Self {
        Self::bytes(status, "text/html; charset=utf-8", body.into_bytes())
    }

    pub fn file(status: u16, file: File, offset: u64, len: u64) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Body::File { file, offset, len },
            content_length: len,
        }
    }

    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
}

pub fn write_response(
    stream: &mut TcpStream,
    keep_alive: bool,
    head_only: bool,
    response: &mut Response,
) -> io::Result<()> {
    let mut head = String::with_capacity(512);
    head.push_str(&format!(
        "HTTP/1.1 {} {}\r\n",
        response.status,
        reason(response.status)
    ));
    head.push_str(&format!("Date: {}\r\n", http_date(Utc::now())));
    head.push_str("Server: LanTransport/1.0\r\n");
    head.push_str(&format!(
        "Connection: {}\r\n",
        if keep_alive { "keep-alive" } else { "close" }
    ));
    head.push_str(&format!("Content-Length: {}\r\n", response.content_length));
    for (name, value) in &response.headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;

    if head_only {
        stream.flush()?;
        return Ok(());
    }

    match &mut response.body {
        Body::Empty => {}
        Body::Bytes(bytes) => stream.write_all(bytes)?,
        Body::File { file, offset, len } => {
            file.seek(SeekFrom::Start(*offset))?;
            let mut remaining = *len;
            let mut buffer = vec![0u8; CHUNK_SIZE];
            while remaining > 0 {
                let want = remaining.min(buffer.len() as u64) as usize;
                let read = file.read(&mut buffer[..want])?;
                if read == 0 {
                    break;
                }
                stream.write_all(&buffer[..read])?;
                remaining -= read as u64;
            }
        }
    }
    stream.flush()
}

pub fn status_line_only(stream: &mut TcpStream, status: u16) -> io::Result<()> {
    let body =
        format!("<!doctype html><meta charset=\"utf-8\"><title>{status}</title><p>{status}</p>");
    let mut response = Response::html(status, body);
    write_response(stream, false, false, &mut response)
}

pub fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        206 => "Partial Content",
        301 => "Moved Permanently",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        411 => "Length Required",
        413 => "Content Too Large",
        414 => "URI Too Long",
        416 => "Range Not Satisfiable",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        505 => "HTTP Version Not Supported",
        _ => "Unknown",
    }
}

pub fn http_date<Tz: chrono::TimeZone>(time: chrono::DateTime<Tz>) -> String {
    time.with_timezone(&Utc)
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string()
}

#[derive(Debug, PartialEq, Eq)]
pub enum RangeOutcome {
    Full,
    Partial { start: u64, end: u64 },
    Unsatisfiable,
}

pub fn parse_range(header: Option<&str>, size: u64) -> RangeOutcome {
    let Some(header) = header else {
        return RangeOutcome::Full;
    };
    let Some(spec) = header.strip_prefix("bytes=") else {
        return RangeOutcome::Full;
    };
    let Some(first) = spec.split(',').next() else {
        return RangeOutcome::Full;
    };
    let first = first.trim();
    let Some((from, to)) = first.split_once('-') else {
        return RangeOutcome::Full;
    };
    if size == 0 {
        return RangeOutcome::Full;
    }

    let (start, end) = if from.is_empty() {
        let Ok(suffix) = to.parse::<u64>() else {
            return RangeOutcome::Full;
        };
        if suffix == 0 {
            return RangeOutcome::Unsatisfiable;
        }
        (size.saturating_sub(suffix), size - 1)
    } else {
        let Ok(start) = from.parse::<u64>() else {
            return RangeOutcome::Full;
        };
        let end = if to.is_empty() {
            size - 1
        } else {
            let Ok(end) = to.parse::<u64>() else {
                return RangeOutcome::Full;
            };
            end.min(size - 1)
        };
        (start, end)
    };

    if start >= size || start > end {
        return RangeOutcome::Unsatisfiable;
    }
    RangeOutcome::Partial { start, end }
}

pub fn split_target(target: &str) -> (&str, &str) {
    let target = match target
        .strip_prefix("http://")
        .or_else(|| target.strip_prefix("https://"))
    {
        Some(rest) => rest.find('/').map_or("/", |index| &rest[index..]),
        None => target,
    };
    target.split_once('?').unwrap_or((target, ""))
}

fn decode_query(value: &str) -> String {
    let value = value.replace('+', " ");
    percent_encoding::percent_decode_str(&value)
        .decode_utf8_lossy()
        .into_owned()
}

pub fn query_pairs(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (decode_query(name), decode_query(value))
        })
        .collect()
}

pub fn query_param(target: &str, name: &str) -> Option<String> {
    let (_, query) = split_target(target);
    query_pairs(query)
        .into_iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
}

pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

const SEGMENT_SET: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

pub fn encode_segment(input: &str) -> String {
    percent_encoding::utf8_percent_encode(input, SEGMENT_SET).to_string()
}

const ATTR_CHAR_SET: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'!')
    .remove(b'#')
    .remove(b'$')
    .remove(b'&')
    .remove(b'+')
    .remove(b'-')
    .remove(b'.')
    .remove(b'^')
    .remove(b'_')
    .remove(b'`')
    .remove(b'|')
    .remove(b'~');

pub fn content_disposition(name: &str) -> String {
    let fallback: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii() && !ch.is_ascii_control() && ch != '"' && ch != '\\' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let encoded = percent_encoding::utf8_percent_encode(name, ATTR_CHAR_SET);
    format!("attachment; filename=\"{fallback}\"; filename*=UTF-8''{encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_forms() {
        assert_eq!(parse_range(None, 100), RangeOutcome::Full);
        assert_eq!(
            parse_range(Some("bytes=0-9"), 100),
            RangeOutcome::Partial { start: 0, end: 9 }
        );
        assert_eq!(
            parse_range(Some("bytes=90-"), 100),
            RangeOutcome::Partial { start: 90, end: 99 }
        );
        assert_eq!(
            parse_range(Some("bytes=-10"), 100),
            RangeOutcome::Partial { start: 90, end: 99 }
        );
        assert_eq!(
            parse_range(Some("bytes=-500"), 100),
            RangeOutcome::Partial { start: 0, end: 99 }
        );
        assert_eq!(
            parse_range(Some("bytes=200-300"), 100),
            RangeOutcome::Unsatisfiable
        );
        assert_eq!(
            parse_range(Some("bytes=9-3"), 100),
            RangeOutcome::Unsatisfiable
        );
        assert_eq!(
            parse_range(Some("bytes=0-9, 20-29"), 100),
            RangeOutcome::Partial { start: 0, end: 9 }
        );
        assert_eq!(parse_range(Some("bytes=abc"), 100), RangeOutcome::Full);
        assert_eq!(parse_range(Some("items=0-9"), 100), RangeOutcome::Full);
        assert_eq!(parse_range(Some("bytes=0-9"), 0), RangeOutcome::Full);
    }

    #[test]
    fn encodes_paths_and_filenames() {
        assert_eq!(encode_segment("a b/c?d"), "a%20b%2Fc%3Fd");
        assert_eq!(
            content_disposition("报告 2026.pdf"),
            "attachment; filename=\"__ 2026.pdf\"; filename*=UTF-8''%E6%8A%A5%E5%91%8A%202026.pdf"
        );
        assert!(content_disposition("a\"b.txt").contains("filename=\"a_b.txt\""));
    }

    #[test]
    fn escapes_html() {
        assert_eq!(
            escape_html("<a href='x'>&</a>"),
            "&lt;a href=&#39;x&#39;&gt;&amp;&lt;/a&gt;"
        );
    }
}
