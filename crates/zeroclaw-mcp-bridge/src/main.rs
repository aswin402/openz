use anyhow::Result;

#[cfg(unix)]
#[tokio::main]
async fn main() -> Result<()> {
    let mut socket_path_arg = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--socket-path" || arg == "-s" {
            socket_path_arg = args.next();
        } else if arg == "--help" || arg == "-h" {
            println!("ZeroClaw MCP IPC Bridge Middleware");
            println!("Usage: zeroclaw-mcp-bridge [options]");
            println!("Options:");
            println!("  -s, --socket-path <path>   Path to the daemon IPC socket");
            println!("  -h, --help                 Show this help message");
            return Ok(());
        }
    }

    let socket_path = if let Some(path) = socket_path_arg {
        std::path::PathBuf::from(path)
    } else if let Ok(env_path) = std::env::var("ZEROCLAW_BRIDGE_SOCKET") {
        std::path::PathBuf::from(env_path)
    } else {
        // Resolve default: ~/.zeroclaw/run/bridge.sock
        if let Some(user_dirs) = directories::UserDirs::new() {
            user_dirs
                .home_dir()
                .join(".zeroclaw")
                .join("run")
                .join("bridge.sock")
        } else {
            return Err(anyhow::anyhow!(
                "Could not determine user home directory for default socket path. Please specify --socket-path."
            ));
        }
    };

    let socket_path = if socket_path.to_string_lossy().starts_with('~') {
        if let Some(user_dirs) = directories::UserDirs::new() {
            let home = user_dirs.home_dir();
            let path_str = socket_path.to_string_lossy();
            if let Some(rest) = path_str.strip_prefix('~') {
                home.join(rest.trim_start_matches(['/', '\\']))
            } else {
                socket_path
            }
        } else {
            socket_path
        }
    } else {
        socket_path
    };

    use serde::Deserialize;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixStream;

    // Connect to UDS
    let mut stream = match UnixStream::connect(&socket_path).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Error: Failed to connect to ZeroClaw bridge socket at {}: {}",
                socket_path.display(),
                e
            );
            eprintln!(
                "Please ensure the ZeroClaw gateway daemon is running and 'bridge_enabled = true' is set in config.toml."
            );
            std::process::exit(1);
        }
    };

    // 1. Handshake
    let hello = serde_json::json!({
        "type": "hello",
        "protocol": "zeroclaw-ipc-v1"
    });
    let hello_str = format!("{}\n", hello);
    stream.write_all(hello_str.as_bytes()).await?;

    let (reader, mut writer) = stream.into_split();
    let mut socket_reader = BufReader::new(reader);
    let mut handshake_line = String::new();
    socket_reader.read_line(&mut handshake_line).await?;

    #[derive(Deserialize)]
    struct HandshakeAck {
        #[serde(rename = "type")]
        msg_type: String,
        status: String,
    }

    let ack: HandshakeAck = match serde_json::from_str(&handshake_line) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Error: Invalid handshake response from daemon: {}", e);
            std::process::exit(1);
        }
    };

    if ack.msg_type != "hello_ack" || ack.status != "ok" {
        eprintln!("Error: Handshake failed, status: {}", ack.status);
        std::process::exit(1);
    }

    // 2. Bidirectional pipe
    let (mut stdin_reader, mut stdout_writer) = (
        tokio::io::BufReader::new(tokio::io::stdin()),
        tokio::io::stdout(),
    );

    // Task 1: stdin -> socket writer
    let mut to_socket_writer = writer;
    let stdin_to_socket = tokio::spawn(async move {
        let mut line = String::new();
        loop {
            line.clear();
            match stdin_reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    if let Err(e) = to_socket_writer.write_all(line.as_bytes()).await {
                        eprintln!("Error writing to socket: {}", e);
                        break;
                    }
                    if let Err(e) = to_socket_writer.flush().await {
                        eprintln!("Error flushing socket: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("Error reading from stdin: {}", e);
                    break;
                }
            }
        }
    });

    // Task 2: socket reader -> stdout
    let socket_to_stdout = tokio::spawn(async move {
        let mut line = String::new();
        loop {
            line.clear();
            match socket_reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    if let Err(e) = stdout_writer.write_all(line.as_bytes()).await {
                        eprintln!("Error writing to stdout: {}", e);
                        break;
                    }
                    if let Err(e) = stdout_writer.flush().await {
                        eprintln!("Error flushing stdout: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("Error reading from socket: {}", e);
                    break;
                }
            }
        }
    });

    // Wait for either direction to end/error
    tokio::select! {
        _ = stdin_to_socket => {},
        _ = socket_to_stdout => {},
    }

    Ok(())
}

#[cfg(not(unix))]
fn main() -> Result<()> {
    eprintln!("Error: The ZeroClaw MCP IPC bridge is only supported on Unix systems.");
    std::process::exit(1);
}
