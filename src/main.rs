mod attention;
mod claude;
mod codex;
mod events;
mod rmk;

use std::{net::SocketAddr, path::PathBuf, time::Duration};

use anyhow::{Result, ensure};
use clap::{Parser, ValueEnum};
use tokio::{net::TcpListener, sync::mpsc};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use attention::AttentionState;
use events::Event;
use rmk::{RmkLighting, Transport};

#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Loopback address for Claude Code HTTP hooks.
    #[arg(long, default_value = "127.0.0.1:37893")]
    listen: SocketAddr,

    /// Explicit Codex app-server WebSocket URL; otherwise discover Codex Desktop through /proc.
    #[arg(long)]
    codex_url: Option<String>,

    /// Path to the RMK-compatible glove80-control executable.
    #[arg(long, default_value = "./bin/glove80-control")]
    glove80_control: PathBuf,

    /// Keyboard transport used by glove80-control.
    #[arg(long, value_enum, default_value_t = TransportArg::Usb)]
    transport: TransportArg,

    /// Overlay lifetime. The daemon refreshes active alerts before this expires.
    #[arg(long, default_value_t = 90_000)]
    overlay_ttl_ms: u32,

    /// How often active alert TTLs are refreshed.
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..))]
    refresh_seconds: u64,

    /// Log lighting commands without changing the keyboard.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum TransportArg {
    Usb,
    Ble,
}

impl From<TransportArg> for Transport {
    fn from(value: TransportArg) -> Self {
        match value {
            TransportArg::Usb => Transport::Usb,
            TransportArg::Ble => Transport::Ble,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let args = Args::parse();
    ensure!(
        u64::from(args.overlay_ttl_ms) > args.refresh_seconds * 1_000,
        "--overlay-ttl-ms must be longer than --refresh-seconds"
    );
    if !args.listen.ip().is_loopback() {
        warn!(listen = %args.listen, "Claude hook server is not restricted to loopback");
    }

    let (event_sender, mut event_receiver) = mpsc::channel(128);
    let codex_events = event_sender.clone();
    let codex_url = args.codex_url.clone();
    tokio::spawn(async move { codex::run(codex_events, codex_url).await });

    let listener = TcpListener::bind(args.listen).await?;
    info!(listen = %args.listen, "Claude hook listener ready");
    let server = axum::serve(listener, claude::router(event_sender));
    let server_handle = tokio::spawn(async move {
        if let Err(error) = server.await {
            warn!(%error, "Claude hook listener stopped");
        }
    });

    let mut state = AttentionState::default();
    let mut lighting = RmkLighting::new(
        args.glove80_control,
        args.transport.into(),
        args.overlay_ttl_ms,
        args.dry_run,
    );
    let mut refresh = tokio::time::interval(Duration::from_secs(args.refresh_seconds));
    refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            Some(event) = event_receiver.recv() => {
                match event {
                    Event::Set { id, source, kind } => state.set(id, source, kind),
                    Event::Remove { id } => state.remove(&id),
                    Event::ReplaceCodex(pending) => state.replace_codex(pending),
                }
                info!(pending = state.len(), "agent attention state changed");
                lighting.reconcile(state.signals(), false).await;
            }
            _ = refresh.tick() => {
                lighting.reconcile(state.signals(), true).await;
            }
            result = tokio::signal::ctrl_c() => {
                result?;
                info!("shutting down");
                break;
            }
        }
    }

    server_handle.abort();
    lighting.clear().await;
    Ok(())
}
