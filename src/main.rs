use std::sync::Arc;
use std::time::SystemTime;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

const PORT: u16 = 8001;
const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024; // 50 MB

fn now() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let addr = format!("0.0.0.0:{PORT}");
    let listener = TcpListener::bind(&addr).await?;
    let file_path = "messages.log";
    println!("Server listening on port {addr} and writing to {file_path}");
    let raw_file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(true)
        .open(file_path)
        .await
        .expect("Failed to open file");
    let previous_bytes_written = raw_file
        .metadata()
        .await
        .expect("Failed to read file length")
        .len();
    if previous_bytes_written > MAX_FILE_SIZE {
        eprintln!("File size exceeds the limit of {MAX_FILE_SIZE} bytes");
        std::process::exit(1);
    }
    let bytes_counter = Arc::new(Mutex::new(previous_bytes_written));
    let file = Arc::new(Mutex::new(BufWriter::new(raw_file)));

    loop {
        let (socket, client) = listener.accept().await?;
        let file_clone = Arc::clone(&file);
        let bytes_counter = Arc::clone(&bytes_counter);
        tokio::spawn(async move {
            let mut reader = BufReader::new(socket);
            let mut buffer = vec![0; 4096];
            let mut write_counter = 0;
            loop {
                let n = match reader.read(&mut buffer).await {
                    Ok(n) if n > 0 => n,
                    Err(_) | Ok(_) => {
                        file_clone
                            .lock()
                            .await
                            .flush()
                            .await
                            .expect("Failed to flush file");
                        break;
                    }
                };
                let line_stamp = format!("$$${}$$${}$$${n}$$$", now(), client);
                {
                    let mut file_guard = file_clone.lock().await;
                    file_guard
                        .write_all(line_stamp.as_bytes())
                        .await
                        .expect("Failed to write to file");
                    file_guard
                        .write_all(&buffer[0..n])
                        .await
                        .expect("Failed to write to file");
                    write_counter += n;
                    if write_counter >= 4096 {
                        file_guard.flush().await.expect("Failed to flush file");
                        write_counter = 0;
                    }
                } // file_guard goes out of scope and releases the lock
                {
                    let mut bytes_guard = bytes_counter.lock().await;
                    if *bytes_guard > MAX_FILE_SIZE {
                        eprintln!("File size exceeds the limit of {MAX_FILE_SIZE} bytes");
                        std::process::exit(1);
                    } else {
                        *bytes_guard += n as u64;
                    }
                } // bytes_guard goes out of scope and releases the lock
            }
        });
    }
}
