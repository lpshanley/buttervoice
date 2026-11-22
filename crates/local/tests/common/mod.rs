//! A loopback file server for download tests.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

pub struct Serve {
    pub status: u16,
    /// Stop after sending this many bytes of the body, as a dropped
    /// connection would.
    pub cut_after: Option<usize>,
    pub honour_ranges: bool,
}

pub struct FileServer {
    pub url: String,
    requests: Receiver<String>,
}

impl FileServer {
    /// Answers one request per entry in `answers`, in order.
    pub fn start(body: Vec<u8>, answers: Vec<Serve>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/model.gguf", listener.local_addr().unwrap());
        let (sent, requests) = mpsc::channel();
        std::thread::spawn(move || {
            for answer in answers {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
                let mut head = String::new();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    head.push_str(&line);
                }
                let from = head
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("range: bytes=")
                            .map(str::to_string)
                    })
                    .and_then(|range| range.trim_end_matches('-').parse::<usize>().ok())
                    .filter(|_| answer.honour_ranges);
                sent.send(head).ok();
                let (status, part) = match (answer.status, from) {
                    (200, Some(from)) => (206, &body[from.min(body.len())..]),
                    (200, None) => (200, &body[..]),
                    (status, _) => (status, &b""[..]),
                };
                let reason = match status {
                    200 => "OK",
                    206 => "Partial Content",
                    _ => "Error",
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    part.len()
                );
                let send = answer
                    .cut_after
                    .map_or(part.len(), |cut| cut.min(part.len()));
                for chunk in part[..send].chunks(8 * 1024) {
                    if stream.write_all(chunk).is_err() {
                        break;
                    }
                }
                let _ = stream.flush();
            }
        });
        Self { url, requests }
    }

    pub fn request(&self) -> String {
        self.requests
            .recv_timeout(Duration::from_secs(5))
            .expect("a request arrived")
    }
}

pub fn ok() -> Serve {
    Serve {
        status: 200,
        cut_after: None,
        honour_ranges: true,
    }
}
