//! A one-shot HTTP server on the loopback interface: it answers a single
//! request with a canned response and hands back what it received.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const WAIT_FOR_REQUEST: Duration = Duration::from_secs(2);

pub struct ReceivedRequest {
    pub request_line: String,
    pub headers: String,
    pub body: Vec<u8>,
}

impl ReceivedRequest {
    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

pub struct TestServer {
    pub base_url: String,
    received: JoinHandle<ReceivedRequest>,
}

impl TestServer {
    pub fn responding(status: u16, body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let received = std::thread::spawn(move || {
            let mut stream = accept_before(&listener, Instant::now() + WAIT_FOR_REQUEST);
            let request = read_request(&mut stream);
            write!(
                stream,
                "HTTP/1.1 {status} Test\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            request
        });
        Self { base_url, received }
    }

    pub fn received(self) -> ReceivedRequest {
        self.received
            .join()
            .expect("no request reached the test server")
    }
}

/// Panics instead of waiting forever, so a client that never calls fails the
/// test instead of hanging it.
fn accept_before(listener: &TcpListener, deadline: Instant) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                return stream;
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "no request reached the test server"
                );
                std::thread::yield_now();
            }
            Err(error) => panic!("the test server could not accept: {error}"),
        }
    }
}

/// A base URL that nothing is listening on.
pub fn unused_base_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    format!("http://{}", listener.local_addr().unwrap())
}

fn read_request(stream: &mut impl Read) -> ReceivedRequest {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut chunk).unwrap();
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(at) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break at;
        }
        assert!(read > 0, "the connection closed before the headers ended");
    };
    let head = String::from_utf8_lossy(&bytes[..header_end]).into_owned();
    let (request_line, headers) = head.split_once("\r\n").unwrap_or((&head, ""));
    let content_length: usize = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().unwrap())
        })
        .unwrap_or(0);
    let mut body = bytes[header_end + 4..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut chunk).unwrap();
        assert!(read > 0, "the connection closed before the body ended");
        body.extend_from_slice(&chunk[..read]);
    }
    ReceivedRequest {
        request_line: request_line.to_string(),
        headers: headers.to_lowercase(),
        body,
    }
}
