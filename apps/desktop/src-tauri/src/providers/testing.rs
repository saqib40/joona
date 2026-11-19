//! A tiny HTTP server for testing the provider clients.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

/// Answers one request with `status` and `body`, and hands back the request.
pub fn serve_once(status: u16, body: &'static str) -> (String, JoinHandle<String>) {
    let (base, server) = serve(vec![(status, body)]);
    let request = std::thread::spawn(move || server.join().unwrap().remove(0));
    (base, request)
}

/// Answers a request with each reply in turn, and hands back the requests.
pub fn serve(replies: Vec<(u16, &'static str)>) -> (String, JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        replies
            .into_iter()
            .map(|(status, body)| {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_request(&mut stream);
                let reply = format!(
                    "HTTP/1.1 {status} Reply\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).unwrap();
                request
            })
            .collect()
    });
    (base, server)
}

fn read_request(stream: &mut impl Read) -> String {
    let mut request = Vec::new();
    let mut buf = [0u8; 16 * 1024];
    let complete = |request: &[u8]| {
        let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
            return false;
        };
        let head = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
        match head
            .lines()
            .find_map(|l| l.strip_prefix("content-length:"))
            .and_then(|v| v.trim().parse::<usize>().ok())
        {
            Some(length) => request.len() >= end + 4 + length,
            None => !head.contains("chunked") || request.ends_with(b"0\r\n\r\n"),
        }
    };
    while !complete(&request) {
        let n = stream.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        request.extend_from_slice(&buf[..n]);
    }
    String::from_utf8_lossy(&request).into_owned()
}

/// The JSON body of a request `serve` handed back.
pub fn json_body(request: &str) -> serde_json::Value {
    serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap_or_default()).unwrap()
}

/// An address where nothing is listening.
pub fn closed_port() -> String {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    format!("http://127.0.0.1:{port}")
}
