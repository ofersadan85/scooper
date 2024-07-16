use std::net::SocketAddr;
use std::process::exit;
use std::sync::Arc;
use std::{env, io};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::{TcpListener, TcpStream};
use tokio::signal::ctrl_c;
use tokio::sync::Mutex;

const DEFAULT_PORT: u16 = 8001;
const DEFAULT_LOG_FILE: &str = "messages.log";
const DEFAULT_MAX_LOG_SIZE: usize = 50 * 1024 * 1024; // 50 MB

use scooper::{human_readable_size, now, parsable_env_var};

async fn increment_bytes_counter(bytes_counter: &Mutex<usize>, n: usize, max_size: usize) -> bool {
    let mut bytes_guard = bytes_counter.lock().await;
    if *bytes_guard > max_size {
        eprintln!(
            "File size exceeds the limit of {} | Exiting...",
            human_readable_size(max_size)
        );
        exit(1);
    } else {
        *bytes_guard += n;
    }
    true
    // bytes_guard goes out of scope and releases the lock
}

async fn log_message(
    file: Arc<Mutex<BufWriter<File>>>,
    socket: &mut TcpStream,
    client: &SocketAddr,
    bytes_counter: Arc<Mutex<usize>>,
    max_size: usize,
) -> io::Result<()> {
    let mut reader = BufReader::new(socket);
    let mut buffer = vec![0; 4096];
    let n = match reader.read(&mut buffer).await {
        Ok(n) if n > 0 => n,
        Err(_) | Ok(_) => {
            // An empty message or an error occurred, we flush what we have and return
            file.lock().await.flush().await?;
            return Ok(());
        }
    };
    let n_fmt = human_readable_size(n);
    println!("Received {n_fmt} from {client}");
    let line_stamp = format!("\n$$${}$$${}$$${n}$$$\n", now(), client);
    {
        let mut file_guard = file.lock().await;
        file_guard.write_all(line_stamp.as_bytes()).await?;
        file_guard.write_all(&buffer[0..n]).await?;
        file_guard.flush().await?;
        // file_guard goes out of scope and releases the lock
    }
    increment_bytes_counter(bytes_counter.as_ref(), n, max_size).await;
    Ok(())
}

async fn graceful_shutdown(
    message: &str,
    code: i32,
    file: Arc<Mutex<BufWriter<File>>>,
    bytes_counter: Arc<Mutex<usize>>,
    original_size: usize,
) {
    file.lock().await.flush().await.unwrap_or_else(|e| {
        eprintln!("Failed to flush log file: {e}");
    });
    println!("{message}");
    let total = *bytes_counter.lock().await;
    println!(
        "Total log size: {} | Written in this session: {}",
        human_readable_size(total),
        human_readable_size(total - original_size)
    );
    exit(code);
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let port = parsable_env_var("PORT", DEFAULT_PORT);
    let log_file = env::var("LOG_FILE").unwrap_or(DEFAULT_LOG_FILE.to_string());
    let max_log_size = parsable_env_var("MAX_FILE_SIZE", DEFAULT_MAX_LOG_SIZE);

    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr).await?;
    println!(
        "Server listening on port {addr} and writing to {log_file} (max file size: {})",
        human_readable_size(max_log_size)
    );
    let raw_file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(true)
        .open(log_file)
        .await?;
    let previous_bytes_written = raw_file.metadata().await?.len() as usize;
    if previous_bytes_written > max_log_size {
        eprintln!("File size exceeds the limit of {max_log_size} bytes | Exiting...");
        exit(1);
    }
    let bytes_counter = Arc::new(Mutex::new(previous_bytes_written));
    let file = Arc::new(Mutex::new(BufWriter::new(raw_file)));

    let file_close = Arc::clone(&file);
    let bytes_close = Arc::clone(&bytes_counter);
    tokio::spawn(async move {
        let caught = ctrl_c().await;
        let (message, code) = match caught {
            Ok(_) => ("Ctrl+C received, shutting down server...".to_string(), 0),
            Err(e) => (format!("Failed to listen for Ctrl+C: {e}"), 1),
        };
        graceful_shutdown(&message, code, file_close, bytes_close, previous_bytes_written).await;
    });

    loop {
        let file = Arc::clone(&file);
        let (mut socket, client) = listener.accept().await?;
        let bytes_counter = Arc::clone(&bytes_counter);
        tokio::spawn(async move {
            log_message(file, &mut socket, &client, bytes_counter, max_log_size)
                .await
                .unwrap_or_else(|e| {
                    eprintln!("Failed to log message from {client}: {e}");
                });
            socket.shutdown().await.unwrap_or_else(|e| {
                eprintln!("Failed to shutdown client socket {client}: {e}");
            });
        });
    }
}
