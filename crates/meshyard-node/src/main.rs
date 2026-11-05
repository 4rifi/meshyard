//! Headless production-core harness. Requests use bounded stdin, never secret CLI arguments.
use meshyard_app::{
    commands, network,
    state::AppState,
    view::{CreateWorkspace, RegisterAgent},
};
use meshyard_core::AgentKind;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Create { name: String, display_name: String },
    Status,
    Start,
    Stop,
    Quit,
    Invite,
    Join { link: String, display_name: String },
    Pairings,
    Confirm { id: String, matches: bool },
    Register { kind: AgentKind, label: String },
}
struct Runtime {
    state: AppState,
}
impl Runtime {
    async fn request(&self, request: Request) -> Result<Value, String> {
        match request {
            Request::Create { name, display_name } => Ok(json!(
                commands::create_workspace(
                    &self.state,
                    CreateWorkspace {
                        name,
                        display_name,
                        mode: meshyard_core::Mode::Strict
                    }
                )
                .await
            )),
            Request::Status => Ok(json!(commands::app_status(&self.state).await)),
            Request::Register { kind, label } => Ok(json!(
                commands::register_agent(&self.state, RegisterAgent { kind, label }).await
            )),
            Request::Start => network::start(&self.state)
                .await
                .map(|v| json!(v))
                .map_err(|_| "Network unavailable".into()),
            Request::Invite => network::invite(&self.state)
                .await
                .map(|v| json!({"link":v}))
                .map_err(|_| "Invite unavailable".into()),
            Request::Join { link, display_name } => network::join(&self.state, link, display_name)
                .await
                .map(|_| json!({"joining":true}))
                .map_err(|_| "Join unavailable".into()),
            Request::Pairings => network::status(&self.state)
                .await
                .map(|v| json!(v))
                .map_err(|_| "Network unavailable".into()),
            Request::Confirm { id, matches } => network::confirm(&self.state, id, matches)
                .await
                .map(|_| json!({"confirmed":matches}))
                .map_err(|_| "Confirmation unavailable".into()),
            Request::Stop | Request::Quit => {
                network::stop(&self.state).await;
                Ok(json!({"stopped":true}))
            }
        }
    }
}
#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        // Fixed errors from our storage/network boundaries contain no secret request data.
        eprintln!("Meshyard node: {error}");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut directory = None;
    let mut file_key = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => directory = args.next().map(PathBuf::from),
            "--key-store" if args.next().as_deref() == Some("file") => file_key = true,
            _ => {
                return Err(
                    "Usage: meshyard-node --data-dir <absolute directory> [--key-store file]"
                        .into(),
                );
            }
        }
    }
    let directory = directory
        .filter(|p| p.is_absolute())
        .ok_or("An absolute data directory is required")?;
    let runtime = Runtime {
        state: AppState::open(directory, file_key).await,
    };
    let mut input = BufReader::new(tokio::io::stdin());
    let mut output = tokio::io::stdout();
    loop {
        // read_until itself is unbounded; fill_buf/consume enforces the cap before allocation.
        let mut line = Vec::new();
        loop {
            let bytes = input.fill_buf().await?;
            if bytes.is_empty() {
                break;
            }
            let count = bytes
                .iter()
                .position(|b| *b == b'\n')
                .map_or(bytes.len(), |n| n + 1);
            if line.len() + count > 16 * 1024 {
                return Err("Request exceeds 16 KiB".into());
            }
            line.extend_from_slice(&bytes[..count]);
            let done = bytes[count - 1] == b'\n';
            input.consume(count);
            if done {
                break;
            }
        }
        if line.is_empty() {
            break;
        }
        let request = serde_json::from_slice::<Request>(&line);
        let quit = matches!(request, Ok(Request::Quit));
        let response = match request {
            Ok(request) => match runtime.request(request).await {
                Ok(value) => json!({"ok":value}),
                Err(error) => json!({"error":error.to_string()}),
            },
            Err(_) => json!({"error":"Invalid request"}),
        };
        output
            .write_all(serde_json::to_string(&response)?.as_bytes())
            .await?;
        output.write_all(b"\n").await?;
        output.flush().await?;
        if quit {
            break;
        }
    }
    runtime
        .state
        .close()
        .await
        .map_err(|_| "Profile could not close")?;
    Ok(())
}
