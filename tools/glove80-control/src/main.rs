use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};

mod config;
mod hostproto;
mod keycodes;
mod keymap;
mod keymapcfg;
mod lightcfg;
mod lighting;
pub mod runtime_manifest;
mod transport;
mod version;

#[derive(Parser)]
#[command(about = "Control Glove80 keyboards over the RMK host protocol \
                   (USB raw HID / BLE): lighting, keymap, persistent config, \
                   version, and bootloader entry")]
struct Cli {
    /// Device to talk to: a /dev/hidraw* path or a BLE address
    /// (AA:BB:CC:DD:EE:FF). Default is auto-discovery.
    #[arg(long, global = true)]
    device: Option<PathBuf>,

    /// Use the USB raw-HID transport.
    #[arg(long, global = true, conflicts_with = "ble")]
    usb: bool,

    /// Use the BLE transport. Default is auto: USB when present, BLE
    /// otherwise.
    #[arg(long, global = true)]
    ble: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage the canonical configuration file — keymap layers + lighting
    /// in one TOML (apply/export/show/validate over host protocol v1.1
    /// lighting sessions and v1.2 keymap writes).
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Reboot a half into its UF2 bootloader (host protocol
    /// ENTER_BOOTLOADER; central half unless --peripheral).
    Bootloader {
        #[command(flatten)]
        host: lighting::BootloaderArgs,
    },
    /// Control the RMK lighting host overlay over USB raw HID or BLE.
    Lighting {
        #[command(subcommand)]
        command: lighting::LightingCommand,
    },
    /// Read and edit the live keymap over USB raw HID or BLE (RMK host
    /// protocol v1.2). Uses the same store Vial edits.
    Keymap {
        #[command(subcommand)]
        command: keymap::KeymapCommand,
    },
    /// Show this CLI's and both keyboard halves' firmware build identity
    /// (RMK host protocol v1.3, GET_VERSION) and warn on mismatched halves.
    Version,
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Parse and semantically validate a configuration file, offline.
    ///
    /// A `.json` file is checked against the legacy runtime keymap schema.
    /// Anything else is treated as the canonical config: TOML with
    /// `[[layer]]` keymap entries and/or lighting tables (start from
    /// `examples/glove80.toml`), or a raw lighting blob, validated with
    /// the exact checks the firmware runs.
    Validate {
        path: PathBuf,
        /// Validate against a target firmware's total layer capacity
        /// (legacy keymap JSON only).
        #[arg(long, value_name = "COUNT")]
        layer_capacity: Option<usize>,
    },
    /// Apply a canonical config file (keymap + lighting) to the keyboard.
    ///
    /// FILE is canonical TOML (start from `examples/glove80.toml`) or a
    /// raw lighting blob (detected by content or a `.bin` extension).
    /// The keymap section is written first via batched KEYMAP_WRITE with
    /// read-back verification — best-effort per batch, NOT atomic across
    /// batches; a failure reports exactly what was written. The lighting
    /// section then goes through one atomic CONFIG session: the device
    /// keeps the complete old lighting config or gets the complete new
    /// one, never a hybrid. Either section may be omitted.
    Apply {
        file: PathBuf,
        /// Validate and print the summary without touching the device.
        #[arg(long)]
        dry_run: bool,
    },
    /// Export the keyboard's active config (keymap + lighting) to a file.
    ///
    /// Writes canonical TOML by default. Layer IDs/names, comments, and
    /// toggle names are host-side only and not stored on the device:
    /// export synthesizes layer ids `layer0..layerN` (position = firmware
    /// slot) and drops trailing all-unbound layers. `--raw` writes the raw
    /// byte-stable lighting blob only (the keymap has no blob form).
    Export {
        file: PathBuf,
        /// Write the raw lighting config blob instead of TOML.
        #[arg(long)]
        raw: bool,
    },
    /// Read the keyboard's active config and print a summary of both the
    /// keymap layers and the lighting records.
    Show,
}

fn hostproto_selector(cli: &Cli) -> transport::Selector {
    let preference = if cli.usb {
        transport::Preference::Usb
    } else if cli.ble {
        transport::Preference::Ble
    } else {
        transport::Preference::Auto
    };
    transport::Selector {
        preference,
        device: cli
            .device
            .as_ref()
            .map(|device| device.to_string_lossy().into_owned()),
    }
}

fn run(cli: Cli) -> Result<()> {
    match &cli.command {
        Command::Config { command } => match command {
            ConfigCommand::Validate {
                path,
                layer_capacity,
            } => {
                // Canonical keymap JSON; everything else is a persistent
                // lighting config (TOML or raw blob).
                let is_json = path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("json"));
                if is_json {
                    let configuration = config::read_and_validate(path, *layer_capacity)?;
                    println!(
                        "Valid schema v{} configuration: {} layers, {} lighting layers, {} toggles",
                        configuration.schema_version,
                        configuration.layers.len(),
                        configuration.lighting_layers.len(),
                        configuration.toggles.len()
                    );
                    return Ok(());
                }
                if layer_capacity.is_some() {
                    bail!("--layer-capacity applies only to canonical keymap JSON files");
                }
                lightcfg::run_validate(path)
            }
            ConfigCommand::Apply { file, dry_run } => {
                lightcfg::run_apply(&hostproto_selector(&cli), file, *dry_run)
            }
            ConfigCommand::Export { file, raw } => {
                lightcfg::run_export(&hostproto_selector(&cli), file, *raw)
            }
            ConfigCommand::Show => lightcfg::run_show(&hostproto_selector(&cli)),
        },
        Command::Lighting { command } => lighting::run(&hostproto_selector(&cli), command),
        Command::Keymap { command } => keymap::run(&hostproto_selector(&cli), command),
        Command::Version => version::run(&hostproto_selector(&cli)),
        Command::Bootloader { host } => {
            lighting::run_bootloader(&hostproto_selector(&cli), host.peripheral, host.yes)
        }
    }
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
