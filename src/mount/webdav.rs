//! Loopback WebDAV bridge: mount a vault without any kernel filesystem.
//!
//! Primary target is macOS (where macFUSE is unavailable): Finder mounts
//! WebDAV natively via `mount volume "http://127.0.0.1:PORT/TOKEN/"`, no
//! kernel extension required. The server binds loopback only and requires a
//! per-session random path token, so other local processes cannot guess the
//! endpoint. All writes go through [`SharedVault`]'s two-phase commit.
//!
//! Safety notes:
//! - LOCK/UNLOCK are acknowledged but not enforced (single-user loopback).
//! - Concurrent edits from Finder and the GUI are serialized by the shared
//!   writer mutex; last commit wins, never a torn container.

use super::shared::SharedVault;
use super::vfs::NodeKind;
use crate::container::ContainerWriter;
use crate::error::{Result, SesameError};
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, MutexGuard};
use std::thread::JoinHandle;
use tiny_http::{Header, Request, Response, Server, StatusCode};

const DAV_XMLNS: &str = "DAV:";
const MAX_PUT_BYTES: usize = 2 * 1024 * 1024 * 1024; // 2 GiB sanity cap

pub(crate) struct WebDavSession {
    shared: Arc<SharedVault>,
    server: Arc<Server>,
    accept_thread: Option<JoinHandle<()>>,
    url: String,
    alive: Arc<AtomicBool>,
}

impl WebDavSession {
    pub(crate) fn start(shared: Arc<SharedVault>) -> Result<Self> {
        let server = Server::http("127.0.0.1:0")
            .map_err(|error| SesameError::Platform(format!("WebDAV bind failed: {error}")))?;
        let port = match server.server_addr() {
            tiny_http::ListenAddr::IP(addr) => addr.port(),
            other => {
                return Err(SesameError::Platform(format!(
                    "unexpected WebDAV listen address: {other:?}"
                )));
            }
        };
        let token = uuid::Uuid::new_v4().simple().to_string();
        let url = format!("http://127.0.0.1:{port}/{token}/");
        let server = Arc::new(server);
        let alive = Arc::new(AtomicBool::new(true));
        let accept_thread = {
            let server = server.clone();
            let alive = alive.clone();
            let shared = shared.clone();
            std::thread::Builder::new()
                .name("sesame-webdav".into())
                .spawn(move || accept_loop(server, shared, token, alive))
                .map_err(|error| {
                    SesameError::Platform(format!("cannot spawn WebDAV thread: {error}"))
                })?
        };
        Ok(Self {
            shared,
            server,
            accept_thread: Some(accept_thread),
            url,
            alive,
        })
    }

    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    pub(crate) fn reload_vfs(&self) -> Result<()> {
        self.shared.reload_vfs()
    }

    pub(crate) fn mutation_lock(&self) -> MutexGuard<'_, ContainerWriter> {
        self.shared.mutation_lock()
    }

    pub(crate) fn ticket(&self) -> super::shared::MutationTicket {
        super::shared::MutationTicket::new(Arc::clone(&self.shared))
    }

    pub(crate) fn stop(&mut self) -> Result<()> {
        self.alive.store(false, Ordering::Relaxed);
        self.server.unblock();
        if let Some(thread) = self.accept_thread.take() {
            let _ = thread.join();
        }
        Ok(())
    }
}

impl Drop for WebDavSession {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn accept_loop(
    server: Arc<Server>,
    shared: Arc<SharedVault>,
    token: String,
    alive: Arc<AtomicBool>,
) {
    while let Ok(request) = server.recv() {
        let shared = shared.clone();
        let token = token.clone();
        std::thread::spawn(move || handle_request(request, shared, &token));
    }
    alive.store(false, Ordering::Relaxed);
}

fn handle_request(mut request: Request, shared: Arc<SharedVault>, token: &str) {
    let method = request.method().as_str().to_string();
    let url = request.url().to_string();
    let ctx = RequestCtx { token };
    let response = match route(&method, &url, &ctx, &shared, &mut request) {
        Ok(response) => response,
        Err(status) => empty_response(status),
    };
    let _ = request.respond(response);
}

fn empty_response(status: StatusCode) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_data(Vec::new()).with_status_code(status)
}

struct RequestCtx<'a> {
    token: &'a str,
}

fn route(
    method: &str,
    url: &str,
    ctx: &RequestCtx,
    shared: &Arc<SharedVault>,
    request: &mut Request,
) -> std::result::Result<Response<std::io::Cursor<Vec<u8>>>, StatusCode> {
    let Some(path) = ctx.decode_path(url) else {
        return Err(StatusCode(404));
    };
    match method {
        "OPTIONS" => Ok(options_response()),
        "PROPFIND" => propfind(&path, request, ctx, shared),
        "GET" => get(&path, shared, false),
        "HEAD" => get(&path, shared, true),
        "PUT" => put(&path, shared, request),
        "MKCOL" => mkcol(&path, shared),
        "DELETE" => delete(&path, shared),
        "MOVE" => moveto(&path, shared, request, ctx),
        "LOCK" => Ok(lock_response()),
        "UNLOCK" => Ok(empty_response(StatusCode(204))),
        _ => Err(StatusCode(405)),
    }
}

impl<'a> RequestCtx<'a> {
    /// Map a request URL to a container path, or `None` when the URL does not
    /// carry the session token or escapes the vault.
    fn decode_path(&self, url: &str) -> Option<String> {
        let path = url.split(['?', '#']).next().unwrap_or("");
        let rest = path.strip_prefix('/')?;
        let (token, rest) = rest.split_once('/').unwrap_or((rest, ""));
        if token != self.token {
            return None;
        }
        let decoded = percent_decode(rest)?;
        let mut segments = Vec::new();
        for segment in decoded.split('/') {
            if segment.is_empty() || segment == "." {
                continue;
            }
            if segment == ".." {
                return None;
            }
            segments.push(segment);
        }
        Some(segments.join("/"))
    }

    fn href(&self, container_path: &str, is_dir: bool) -> String {
        let mut href = format!("/{}", self.token);
        if !container_path.is_empty() {
            for segment in container_path.split('/') {
                href.push('/');
                href.push_str(&percent_encode(segment));
            }
        }
        if is_dir {
            href.push('/');
        }
        href
    }
}

fn options_response() -> Response<std::io::Cursor<Vec<u8>>> {
    let allow = "OPTIONS, PROPFIND, GET, HEAD, PUT, MKCOL, DELETE, MOVE, LOCK, UNLOCK";
    empty_response(StatusCode(200))
        .with_header(header("DAV", "1, 2"))
        .with_header(header("Allow", allow))
        .with_header(header("MS-Author-Via", "DAV"))
}

fn propfind(
    path: &str,
    request: &Request,
    ctx: &RequestCtx,
    shared: &Arc<SharedVault>,
) -> std::result::Result<Response<std::io::Cursor<Vec<u8>>>, StatusCode> {
    let depth = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Depth"))
        .map(|header| header.value.as_str().to_string())
        .unwrap_or_else(|| "infinity".into());
    if depth == "infinity" {
        // RFC 4918: servers may reject infinite-depth PROPFIND.
        return Err(StatusCode(403));
    }

    let Some(metadata) = shared.metadata_of(path) else {
        return Err(StatusCode(404));
    };
    let mut body = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<D:multistatus xmlns:D=\"DAV:\">\n",
    );
    push_propstat(
        &mut body,
        ctx,
        path,
        metadata.kind == NodeKind::Directory,
        metadata.size,
        metadata.modified_at,
        metadata.created_at,
    );
    if metadata.kind == NodeKind::Directory
        && depth != "0"
        && let Some(children) = shared.list_dir(path)
    {
        let vfs = shared.vfs();
        for child in children {
            let child_path = if path.is_empty() {
                child.name.clone()
            } else {
                format!("{path}/{}", child.name)
            };
            let (size, modified, created) = vfs
                .metadata(child.inode)
                .map(|meta| (meta.size, meta.modified_at, meta.created_at))
                .unwrap_or((0, 0, 0));
            push_propstat(
                &mut body,
                ctx,
                &child_path,
                child.kind == NodeKind::Directory,
                size,
                modified,
                created,
            );
        }
    }
    body.push_str("</D:multistatus>");
    Ok(Response::from_data(body.into_bytes())
        .with_status_code(StatusCode(207))
        .with_header(header("Content-Type", "application/xml; charset=utf-8")))
}

fn push_propstat(
    body: &mut String,
    ctx: &RequestCtx,
    path: &str,
    is_dir: bool,
    size: u64,
    modified: u64,
    created: u64,
) {
    let name = path.rsplit('/').next().unwrap_or(path);
    body.push_str("<D:response><D:href>");
    body.push_str(&xml_escape(&ctx.href(path, is_dir)));
    body.push_str("</D:href><D:propstat><D:prop>");
    body.push_str("<D:displayname>");
    body.push_str(&xml_escape(name));
    body.push_str("</D:displayname>");
    if is_dir {
        body.push_str("<D:resourcetype><D:collection/></D:resourcetype>");
    } else {
        body.push_str("<D:resourcetype/>");
        body.push_str(&format!("<D:getcontentlength>{size}</D:getcontentlength>"));
        body.push_str("<D:getcontenttype>application/octet-stream</D:getcontenttype>");
    }
    let mtime = httpdate::fmt_http_date(
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(modified.max(created)),
    );
    body.push_str(&format!("<D:getlastmodified>{mtime}</D:getlastmodified>"));
    body.push_str("<D:ishidden>0</D:ishidden>");
    body.push_str("</D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>\n");
}

fn get(
    path: &str,
    shared: &Arc<SharedVault>,
    head_only: bool,
) -> std::result::Result<Response<std::io::Cursor<Vec<u8>>>, StatusCode> {
    let Some(metadata) = shared.metadata_of(path) else {
        return Err(StatusCode(404));
    };
    if metadata.kind == NodeKind::Directory {
        return Err(StatusCode(405));
    }
    let data = if head_only {
        Vec::new()
    } else {
        shared.read_file(path).map_err(|_| StatusCode(500))?
    };
    let response = Response::from_data(data)
        .with_status_code(StatusCode(200))
        .with_header(header("Content-Type", "application/octet-stream"))
        .with_header(header("Content-Length", &metadata.size.to_string()))
        .with_header(header("Accept-Ranges", "bytes"))
        .with_header(header(
            "ETag",
            &format!("\"{}-{}\"", metadata.modified_at, metadata.size),
        ));
    Ok(response)
}

fn put(
    path: &str,
    shared: &Arc<SharedVault>,
    request: &mut Request,
) -> std::result::Result<Response<std::io::Cursor<Vec<u8>>>, StatusCode> {
    if path.is_empty() {
        return Err(StatusCode(405));
    }
    let existed = shared.metadata_of(path).is_some();
    if shared
        .metadata_of(path)
        .is_some_and(|meta| meta.kind == NodeKind::Directory)
    {
        return Err(StatusCode(405));
    }
    let content_length = request.body_length();
    if content_length.is_some_and(|len| len > MAX_PUT_BYTES) {
        return Err(StatusCode(413));
    }
    let mut data = Vec::new();
    let reader = request.as_reader();
    reader
        .take(MAX_PUT_BYTES as u64)
        .read_to_end(&mut data)
        .map_err(|_| StatusCode(400))?;
    shared
        .write_file(path, &data)
        .map_err(|error| match error {
            SesameError::DiskFull { .. } => StatusCode(507),
            _ => StatusCode(500),
        })?;
    Ok(empty_response(if existed {
        StatusCode(204)
    } else {
        StatusCode(201)
    }))
}

fn mkcol(
    path: &str,
    shared: &Arc<SharedVault>,
) -> std::result::Result<Response<std::io::Cursor<Vec<u8>>>, StatusCode> {
    if path.is_empty() {
        return Err(StatusCode(405));
    }
    if shared.metadata_of(path).is_some() {
        return Err(StatusCode(405));
    }
    shared.mkdir(path).map_err(|_| StatusCode(409))?;
    Ok(empty_response(StatusCode(201)))
}

fn delete(
    path: &str,
    shared: &Arc<SharedVault>,
) -> std::result::Result<Response<std::io::Cursor<Vec<u8>>>, StatusCode> {
    if path.is_empty() {
        return Err(StatusCode(403));
    }
    if shared.metadata_of(path).is_none() {
        return Err(StatusCode(404));
    }
    shared.remove(path).map_err(|_| StatusCode(500))?;
    Ok(empty_response(StatusCode(204)))
}

fn moveto(
    path: &str,
    shared: &Arc<SharedVault>,
    request: &Request,
    ctx: &RequestCtx,
) -> std::result::Result<Response<std::io::Cursor<Vec<u8>>>, StatusCode> {
    if path.is_empty() {
        return Err(StatusCode(403));
    }
    let Some(destination) = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Destination"))
        .map(|header| header.value.as_str().to_string())
    else {
        return Err(StatusCode(400));
    };
    let overwrite = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Overwrite"))
        .is_none_or(|header| header.value.as_str() != "F");
    // Destination may be an absolute URL or an absolute path.
    let dest_path_part = destination
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split_once('/').map(|(_, path)| format!("/{path}")))
        .unwrap_or(destination);
    let Some(dest) = ctx.decode_path(&dest_path_part) else {
        return Err(StatusCode(400));
    };
    if shared.metadata_of(path).is_none() {
        return Err(StatusCode(404));
    }
    let existed = shared.metadata_of(&dest).is_some();
    if existed && !overwrite {
        return Err(StatusCode(412));
    }
    if existed
        && shared
            .metadata_of(&dest)
            .is_some_and(|meta| meta.kind == NodeKind::Directory)
    {
        return Err(StatusCode(403));
    }
    shared
        .rename(path, &dest, overwrite)
        .map_err(|_| StatusCode(409))?;
    Ok(empty_response(if existed {
        StatusCode(204)
    } else {
        StatusCode(201)
    }))
}

fn lock_response() -> Response<std::io::Cursor<Vec<u8>>> {
    let token = format!("opaquelocktoken:{}", uuid::Uuid::new_v4());
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<D:prop xmlns:D=\"{DAV_XMLNS}\"><D:lockdiscovery><D:activelock>\
<D:locktype><D:write/></D:locktype><D:lockscope><D:exclusive/></D:lockscope><D:depth>infinity</D:depth>\
<D:owner><D:href>closed-sesame</D:href></D:owner><D:timeout>Second-3600</D:timeout>\
<D:locktoken><D:href>{token}</D:href></D:locktoken></D:activelock></D:lockdiscovery></D:prop>"
    );
    Response::from_data(body.into_bytes())
        .with_status_code(StatusCode(200))
        .with_header(header("Content-Type", "application/xml; charset=utf-8"))
        .with_header(header("Lock-Token", &format!("<{token}>")))
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header bytes")
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 > bytes.len() - 1 {
                return None;
            }
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
            let value = u8::from_str_radix(hex, 16).ok()?;
            output.push(value);
            index += 3;
        } else if bytes[index] == b'+' {
            output.push(b' ');
            index += 1;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).ok()
}

fn percent_encode(input: &str) -> String {
    let mut output = String::new();
    for byte in input.bytes() {
        let unreserved = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~');
        if unreserved {
            output.push(byte as char);
        } else {
            output.push_str(&format!("%{byte:02X}"));
        }
    }
    output
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_roundtrip() {
        assert_eq!(percent_decode("a%20b/c%2Bd"), Some("a b/c+d".into()));
        assert_eq!(percent_encode("a b+c"), "a%20b%2Bc");
        assert_eq!(percent_decode("%zz"), None);
        assert_eq!(percent_decode("%4"), None);
    }

    #[test]
    fn decode_path_requires_token_and_rejects_traversal() {
        let ctx = RequestCtx { token: "abc" };
        assert_eq!(
            ctx.decode_path("/abc/docs/a.txt"),
            Some("docs/a.txt".into())
        );
        assert_eq!(ctx.decode_path("/abc/"), Some(String::new()));
        assert_eq!(ctx.decode_path("/abc"), Some(String::new()));
        assert_eq!(ctx.decode_path("/wrong/docs"), None);
        assert_eq!(ctx.decode_path("/abc/../etc"), None);
        assert_eq!(ctx.decode_path("/abc/a%2Fb.txt"), Some("a/b.txt".into()));
    }

    #[test]
    fn href_encodes_segments_and_marks_dirs() {
        let ctx = RequestCtx { token: "abc" };
        assert_eq!(ctx.href("docs/a b.txt", false), "/abc/docs/a%20b.txt");
        assert_eq!(ctx.href("docs", true), "/abc/docs/");
        assert_eq!(ctx.href("", true), "/abc/");
    }
}
