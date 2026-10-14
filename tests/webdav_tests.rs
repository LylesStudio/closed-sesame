//! End-to-end tests for the loopback WebDAV bridge.
//!
//! Speaks raw HTTP/1.1 over `std::net::TcpStream` — no external client
//! needed — and verifies the full writable closed loop against the
//! container afterwards.

#![cfg(all(feature = "mount", not(target_os = "windows")))]

use closed_sesame::app::credentials::Credential;
use closed_sesame::container::ContainerWriter;
use closed_sesame::crypto::{KdfConfig, Salt};
use closed_sesame::keymgmt::KeyEncryptingKey;
use std::io::{Read, Write};
use std::net::TcpStream;
use tempfile::TempDir;

struct Bridge {
    mounted: closed_sesame::mount::MountedVault,
    port: u16,
    token: String,
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = self.mounted.unmount();
    }
}

fn start_bridge(
    dir: &TempDir,
    files: &[(&str, &[u8])],
) -> (Bridge, KeyEncryptingKey, std::path::PathBuf) {
    let container = dir.path().join("dav.sesame");
    let password = "dav-pass";
    let salt = Salt::generate();
    let kek =
        KeyEncryptingKey::from_password(password, &salt, KdfConfig::default_password()).unwrap();
    let mut writer = ContainerWriter::create(&container, &kek, salt, 8 * 1024 * 1024).unwrap();
    for (name, content) in files {
        let src = dir.path().join(name.replace('/', "_"));
        std::fs::write(&src, content).unwrap();
        writer.add_file(&src, name).unwrap();
    }
    drop(writer);

    // The WebDAV bridge is exercised through mount_webdav_with_kek; prepare
    // the KEK the same way the CLI does.
    let kek = closed_sesame::app::credentials::prepare_credentials(
        &container,
        Credential::Password(password),
    )
    .unwrap();
    let mounted = closed_sesame::mount::mount_webdav_with_kek(&kek, &container).unwrap();
    let url = mounted.mount_point().to_string_lossy().into_owned();
    // http://127.0.0.1:PORT/TOKEN/
    let without_scheme = url.strip_prefix("http://").expect("http url");
    let (host, rest) = without_scheme.split_once('/').expect("url path");
    let port: u16 = host.split(':').nth(1).unwrap().parse().unwrap();
    let token = rest.trim_matches('/').to_string();
    (
        Bridge {
            mounted,
            port,
            token,
        },
        kek,
        container,
    )
}

struct HttpResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpResponse {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

fn http(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> HttpResponse {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).unwrap();
    stream.write_all(body).unwrap();

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let header_end = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("header terminator");
    let head = String::from_utf8_lossy(&raw[..header_end]).into_owned();
    let mut lines = head.lines();
    let status: u16 = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .expect("status code");
    let headers = lines
        .filter_map(|line| {
            line.split_once(':')
                .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        })
        .collect();
    let body = raw[header_end + 4..].to_vec();
    HttpResponse {
        status,
        headers,
        body,
    }
}

#[test]
fn webdav_full_writable_cycle() {
    let dir = TempDir::new().unwrap();
    let (mut bridge, kek, container) = start_bridge(&dir, &[("hello.txt", b"hello vault")]);
    let base = format!("/{}", bridge.token);

    // OPTIONS advertises WebDAV.
    let options = http(bridge.port, "OPTIONS", &base, &[], &[]);
    assert_eq!(options.status, 200);
    assert!(options.header("DAV").unwrap().contains('2'));

    // PROPFIND depth 1 lists the seed file.
    let list = http(bridge.port, "PROPFIND", &base, &[("Depth", "1")], &[]);
    assert_eq!(list.status, 207, "{}", String::from_utf8_lossy(&list.body));
    let xml = String::from_utf8_lossy(&list.body).into_owned();
    assert!(xml.contains("hello.txt"), "{xml}");
    assert!(xml.contains("<D:multistatus"), "{xml}");

    // GET returns decrypted content.
    let get = http(bridge.port, "GET", &format!("{base}/hello.txt"), &[], &[]);
    assert_eq!(get.status, 200);
    assert_eq!(get.body, b"hello vault");

    // PUT creates a new file (201), GET reads it back.
    let put = http(
        bridge.port,
        "PUT",
        &format!("{base}/created.txt"),
        &[],
        b"born over webdav",
    );
    assert_eq!(put.status, 201);
    let get_created = http(bridge.port, "GET", &format!("{base}/created.txt"), &[], &[]);
    assert_eq!(get_created.body, b"born over webdav");

    // PUT over an existing file replaces it (204), never corrupts.
    let put_over = http(
        bridge.port,
        "PUT",
        &format!("{base}/hello.txt"),
        &[],
        b"replaced",
    );
    assert_eq!(put_over.status, 204);
    let get_replaced = http(bridge.port, "GET", &format!("{base}/hello.txt"), &[], &[]);
    assert_eq!(get_replaced.body, b"replaced");

    // MKCOL + nested PUT + MOVE rename.
    let mkcol = http(bridge.port, "MKCOL", &format!("{base}/docs"), &[], &[]);
    assert_eq!(mkcol.status, 201);
    let put_nested = http(
        bridge.port,
        "PUT",
        &format!("{base}/docs/a.txt"),
        &[],
        b"nested",
    );
    assert_eq!(put_nested.status, 201);
    let moved = http(
        bridge.port,
        "MOVE",
        &format!("{base}/docs/a.txt"),
        &[("Destination", &format!("{base}/docs/b.txt"))],
        &[],
    );
    assert!(
        moved.status == 201 || moved.status == 204,
        "MOVE: {}",
        moved.status
    );
    let get_moved = http(bridge.port, "GET", &format!("{base}/docs/b.txt"), &[], &[]);
    assert_eq!(get_moved.body, b"nested");

    // LOCK / UNLOCK are acknowledged.
    let lock = http(bridge.port, "LOCK", &format!("{base}/docs/b.txt"), &[], &[]);
    assert_eq!(lock.status, 200);
    assert!(lock.header("Lock-Token").is_some());
    let unlock = http(
        bridge.port,
        "UNLOCK",
        &format!("{base}/docs/b.txt"),
        &[],
        &[],
    );
    assert_eq!(unlock.status, 204);

    // DELETE removes.
    let deleted = http(
        bridge.port,
        "DELETE",
        &format!("{base}/docs/b.txt"),
        &[],
        &[],
    );
    assert_eq!(deleted.status, 204);
    let gone = http(bridge.port, "GET", &format!("{base}/docs/b.txt"), &[], &[]);
    assert_eq!(gone.status, 404);

    // Unknown token is rejected.
    let wrong = http(bridge.port, "GET", "/not-the-token/hello.txt", &[], &[]);
    assert_eq!(wrong.status, 404);

    bridge.mounted.unmount().unwrap();

    // The container itself holds the final state.
    let reader = closed_sesame::ContainerReader::open(&container, &kek).unwrap();
    let entries = reader.list_files().unwrap();
    let paths: Vec<&str> = entries.iter().map(|entry| entry.path.as_str()).collect();
    assert!(paths.contains(&"hello.txt"), "{paths:?}");
    assert!(paths.contains(&"created.txt"), "{paths:?}");
    assert!(paths.contains(&"docs"), "{paths:?}");
    assert!(!paths.contains(&"docs/b.txt"), "{paths:?}");
    let hello = entries
        .iter()
        .find(|entry| entry.path == "hello.txt")
        .unwrap();
    assert_eq!(reader.read_file(hello).unwrap(), b"replaced");
}

#[test]
fn webdav_percent_encoded_paths() {
    let dir = TempDir::new().unwrap();
    let (bridge, _kek, _container) = start_bridge(&dir, &[("my file.txt", b"spaced")]);
    let base = format!("/{}", bridge.token);
    let get = http(
        bridge.port,
        "GET",
        &format!("{base}/my%20file.txt"),
        &[],
        &[],
    );
    assert_eq!(get.status, 200);
    assert_eq!(get.body, b"spaced");
}
