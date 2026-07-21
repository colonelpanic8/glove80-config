use std::fs;

use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::{sync::mpsc, time::Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, info, warn};

use crate::{attention::Kind, events::Event};

const SOURCE_KINDS: &[&str] = &[
    "cli",
    "vscode",
    "exec",
    "appServer",
    "subAgent",
    "subAgentReview",
    "subAgentCompact",
    "subAgentThreadSpawn",
    "subAgentOther",
    "unknown",
];

pub async fn run(events: mpsc::Sender<Event>, configured_url: Option<String>) {
    loop {
        let url = match configured_url.clone().or_else(discover_codex_url) {
            Some(url) => url,
            None => {
                debug!("no Codex Desktop app-server found");
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };

        info!(%url, "connecting to Codex app-server");
        if let Err(error) = run_connection(&url, &events).await {
            warn!(%url, %error, "Codex app-server connection ended");
        }
        let _ = events.send(Event::ReplaceCodex(Vec::new())).await;
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn run_connection(url: &str, events: &mpsc::Sender<Event>) -> Result<()> {
    let (socket, _) = connect_async(url)
        .await
        .with_context(|| format!("could not connect to {url}"))?;
    let (mut writer, mut reader) = socket.split();

    writer
        .send(Message::Text(
            json!({
                "method": "initialize",
                "id": 0,
                "params": {
                    "clientInfo": {
                        "name": "rmk_agent_attention",
                        "title": "RMK Agent Attention",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            })
            .to_string()
            .into(),
        ))
        .await?;

    let mut initialized = false;
    while let Some(message) = reader.next().await {
        match message? {
            Message::Text(text) => {
                let value: Value = serde_json::from_str(&text)?;
                if value.get("id") == Some(&json!(0)) && !initialized {
                    if value.get("error").is_some() {
                        bail!("Codex initialize failed: {value}");
                    }
                    writer
                        .send(Message::Text(
                            json!({ "method": "initialized", "params": {} })
                                .to_string()
                                .into(),
                        ))
                        .await?;
                    writer
                        .send(Message::Text(
                            json!({
                                "method": "thread/list",
                                "id": 1,
                                "params": {
                                    "limit": 500,
                                    "sourceKinds": SOURCE_KINDS,
                                    "useStateDbOnly": true
                                }
                            })
                            .to_string()
                            .into(),
                        ))
                        .await?;
                    initialized = true;
                    continue;
                }
                handle_message(&value, events).await?;
            }
            Message::Ping(payload) => writer.send(Message::Pong(payload)).await?,
            Message::Close(_) => break,
            _ => {}
        }
    }
    Ok(())
}

async fn handle_message(value: &Value, events: &mpsc::Sender<Event>) -> Result<()> {
    if value.get("id") == Some(&json!(1)) {
        let pending = value
            .pointer("/result/data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|thread| {
                let id = thread.get("id")?.as_str()?;
                attention_kind(thread.get("status")?).map(|kind| (format!("codex:{id}"), kind))
            })
            .collect();
        events.send(Event::ReplaceCodex(pending)).await.ok();
        return Ok(());
    }

    if value.get("method").and_then(Value::as_str) == Some("thread/status/changed") {
        let Some(thread_id) = value.pointer("/params/threadId").and_then(Value::as_str) else {
            return Ok(());
        };
        let id = format!("codex:{thread_id}");
        let event = value
            .pointer("/params/status")
            .and_then(attention_kind)
            .map_or(Event::Remove { id: id.clone() }, |kind| Event::Set {
                id,
                source: crate::attention::Source::Codex,
                kind,
            });
        events.send(event).await.ok();
    }
    Ok(())
}

fn attention_kind(status: &Value) -> Option<Kind> {
    if status.get("type").and_then(Value::as_str) != Some("active") {
        return None;
    }
    let flags = status.get("activeFlags")?.as_array()?;
    if flags.iter().any(|flag| flag == "waitingOnApproval") {
        Some(Kind::Approval)
    } else if flags.iter().any(|flag| flag == "waitingOnUserInput") {
        Some(Kind::Input)
    } else {
        None
    }
}

pub fn discover_codex_url() -> Option<String> {
    let mut candidates = Vec::new();
    for entry in fs::read_dir("/proc").ok()?.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(cmdline) = fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let args: Vec<_> = cmdline
            .split(|byte| *byte == 0)
            .filter(|arg| !arg.is_empty())
            .map(|arg| String::from_utf8_lossy(arg).into_owned())
            .collect();
        if let Some(url) = remote_control_url(&args) {
            candidates.push((pid, url));
        }
    }
    candidates.sort_by_key(|(pid, _)| *pid);
    candidates.pop().map(|(_, url)| url)
}

fn remote_control_url(args: &[String]) -> Option<String> {
    if !args.iter().any(|arg| arg == "app-server")
        || !args.iter().any(|arg| arg == "--remote-control")
    {
        return None;
    }
    args.windows(2)
        .find(|pair| pair[0] == "--listen" && is_loopback_websocket(&pair[1]))
        .map(|pair| pair[1].clone())
}

fn is_loopback_websocket(url: &str) -> bool {
    url.strip_prefix("ws://127.0.0.1:")
        .is_some_and(|port| port.parse::<u16>().is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn finds_desktop_remote_control_listener() {
        let command = args(&[
            "/nix/store/example/bin/codex-raw",
            "app-server",
            "--remote-control",
            "--listen",
            "ws://127.0.0.1:46231",
        ]);
        assert_eq!(
            remote_control_url(&command).as_deref(),
            Some("ws://127.0.0.1:46231")
        );
    }

    #[test]
    fn rejects_non_remote_and_non_loopback_servers() {
        assert!(
            remote_control_url(&args(&[
                "codex",
                "app-server",
                "--listen",
                "ws://127.0.0.1:1"
            ]))
            .is_none()
        );
        assert!(
            remote_control_url(&args(&[
                "codex",
                "app-server",
                "--remote-control",
                "--listen",
                "ws://0.0.0.0:1"
            ]))
            .is_none()
        );
    }

    #[test]
    fn maps_codex_flags_to_attention() {
        assert_eq!(
            attention_kind(&json!({
                "type": "active",
                "activeFlags": ["waitingOnUserInput"]
            })),
            Some(Kind::Input)
        );
        assert_eq!(
            attention_kind(&json!({
                "type": "active",
                "activeFlags": ["waitingOnUserInput", "waitingOnApproval"]
            })),
            Some(Kind::Approval)
        );
        assert_eq!(attention_kind(&json!({ "type": "idle" })), None);
    }
}
