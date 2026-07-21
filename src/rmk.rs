use std::{path::PathBuf, process::Stdio};

use anyhow::{Context, Result, bail};
use tokio::process::Command;
use tracing::{info, warn};

use crate::attention::{ATTENTION_LEDS, Signal};

pub struct RmkLighting {
    executable: PathBuf,
    transport: Transport,
    ttl_ms: u32,
    dry_run: bool,
    last_desired: [Option<Signal>; 3],
}

#[derive(Clone, Copy, Debug)]
pub enum Transport {
    Usb,
    Ble,
}

impl RmkLighting {
    pub fn new(executable: PathBuf, transport: Transport, ttl_ms: u32, dry_run: bool) -> Self {
        Self {
            executable,
            transport,
            ttl_ms,
            dry_run,
            last_desired: [None, None, None],
        }
    }

    pub async fn reconcile(&mut self, desired: [Option<Signal>; 3], refresh: bool) {
        for (index, signal) in desired.iter().enumerate() {
            if self.last_desired[index] == *signal && (!refresh || signal.is_none()) {
                continue;
            }
            let result = match signal {
                Some(signal) => self.set(*signal).await,
                None => self.unset(ATTENTION_LEDS[index]).await,
            };
            match result {
                Ok(()) => self.last_desired[index] = *signal,
                Err(error) => {
                    warn!(led = ATTENTION_LEDS[index], %error, "could not update RMK attention LED");
                }
            }
        }
    }

    pub async fn clear(&mut self) {
        self.reconcile([None, None, None], false).await;
    }

    async fn set(&self, signal: Signal) -> Result<()> {
        let mut args = vec![
            self.transport_arg().to_owned(),
            "lighting".to_owned(),
            "set".to_owned(),
            signal.led.to_string(),
            signal.color.to_owned(),
            "--effect".to_owned(),
            signal.effect.to_owned(),
            "--period".to_owned(),
            signal.period_ms.to_string(),
            "--ttl".to_owned(),
            self.ttl_ms.to_string(),
        ];
        if let Some(duty) = signal.duty_percent {
            args.extend(["--duty".to_owned(), duty.to_string()]);
        }
        self.run(args).await
    }

    async fn unset(&self, led: u8) -> Result<()> {
        self.run(vec![
            self.transport_arg().to_owned(),
            "lighting".to_owned(),
            "unset".to_owned(),
            led.to_string(),
        ])
        .await
    }

    fn transport_arg(&self) -> &'static str {
        match self.transport {
            Transport::Usb => "--usb",
            Transport::Ble => "--ble",
        }
    }

    async fn run(&self, args: Vec<String>) -> Result<()> {
        if self.dry_run {
            info!(command = %self.executable.display(), ?args, "dry-run RMK lighting update");
            return Ok(());
        }
        let status = Command::new(&self.executable)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .await
            .with_context(|| format!("could not run {}", self.executable.display()))?;
        if !status.success() {
            bail!("{} exited with {status}", self.executable.display());
        }
        Ok(())
    }
}
