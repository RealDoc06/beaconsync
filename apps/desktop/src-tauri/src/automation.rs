//! Scripting hook for demos and UI tests, compiled only with `--features automation`.
//! POST JavaScript to http://127.0.0.1:47555 and it runs in the main window. Localhost only,
//! and never part of the builds we ship.

use std::io::{BufRead, BufReader, Read, Write};
use tauri::{AppHandle, Manager};

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let Ok(listener) = std::net::TcpListener::bind("127.0.0.1:47555") else { return };
        for mut stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(&stream);
            let mut len = 0usize;
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok_and(|n| n > 0) && line != "\r\n" {
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
                line.clear();
            }
            let mut body = vec![0; len];
            if reader.read_exact(&mut body).is_ok()
                && let Some(w) = app.get_webview_window("main")
            {
                w.eval(String::from_utf8_lossy(&body)).ok();
            }
            stream.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").ok();
        }
    });
}
