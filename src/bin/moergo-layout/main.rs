//! Offline analysis and transforms for MoErgo RMK runtime configuration
//! TOML files (`config/glove80.toml`-style). Everything here works without
//! a connected keyboard; apply results through the usual
//! `moergo-control config diff/apply` workflow.

mod address;
mod model;
mod preset;
mod transform;
mod usage;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "moergo-layout",
    about = "Analyze and transform MoErgo RMK runtime configuration TOML offline"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Report behavior usage: layer activators and reachability, morse,
    /// macro, fork, and morse-profile references, orphans, and dangling
    /// references
    Usage {
        /// Runtime config TOML files to analyze
        #[arg(required = true)]
        configs: Vec<PathBuf>,
        /// Also print a histogram of base keycode usage
        #[arg(long)]
        keycodes: bool,
        /// Exit non-zero if any warning is produced
        #[arg(long)]
        check: bool,
    },
    /// Generate the other OS's variant of a config by swapping Ctrl and
    /// GUI in every action binding (grids, binds, morses, combos, forks,
    /// macros). The swap is its own inverse: applying it to the generated
    /// variant returns the original bindings.
    Os {
        /// Which variant to produce; both perform the same Ctrl/GUI swap,
        /// the name only documents intent
        scheme: OsScheme,
        /// Source runtime config TOML (canonical for the *other* OS)
        config: PathBuf,
        /// Write here instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Generate an alternate alpha layout (Colemak, Colemak-DH, Dvorak)
    /// from a QWERTY source config. Only the default layer is remapped
    /// unless --layers is given; positional layers like Games should stay
    /// unlisted.
    Alpha {
        layout: AlphaLayout,
        /// Source runtime config TOML with QWERTY alphas
        config: PathBuf,
        /// Write here instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Comma-separated layer indices to remap (default: the default layer)
        #[arg(long, value_delimiter = ',')]
        layers: Option<Vec<usize>>,
    },
    /// Apply or inspect a partial preset (fragment): new layers, morse
    /// entries, combos, macros, morse profiles, and lighting scene cells,
    /// plus sparse patches to existing keys. `$name` references resolve to
    /// concrete slots at apply time.
    Preset {
        #[command(subcommand)]
        command: PresetCommand,
    },
}

#[derive(Subcommand)]
enum PresetCommand {
    /// Merge a preset into a runtime config and write the result
    Apply {
        /// Preset TOML (see docs/layout-tools.md for the format)
        preset: PathBuf,
        /// Target runtime config TOML
        config: PathBuf,
        /// Write here instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Summarize what a preset would add, without a target config
    Show {
        /// Preset TOML
        preset: PathBuf,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum OsScheme {
    Mac,
    Pc,
}

#[derive(Clone, Copy, ValueEnum)]
enum AlphaLayout {
    Colemak,
    #[value(name = "colemak-dh")]
    ColemakDh,
    Dvorak,
}

impl AlphaLayout {
    fn as_str(self) -> &'static str {
        match self {
            AlphaLayout::Colemak => "colemak",
            AlphaLayout::ColemakDh => "colemak-dh",
            AlphaLayout::Dvorak => "dvorak",
        }
    }
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Usage {
            configs,
            keycodes,
            check,
        } => {
            let mut total_warnings = 0usize;
            for path in &configs {
                let config = model::load(path)?;
                let report = usage::analyze(&config);
                println!("== {} ==", path.display());
                print!("{}", report.text);
                if keycodes {
                    println!("\nKeycode usage:");
                    for (keycode, count) in usage::keycode_histogram(&config) {
                        println!("  {count:>3}  {keycode}");
                    }
                }
                if report.warnings.is_empty() {
                    println!("\nNo warnings.");
                } else {
                    println!("\nWarnings:");
                    for warning in &report.warnings {
                        println!("  - {warning}");
                    }
                }
                println!();
                total_warnings += report.warnings.len();
            }
            if check && total_warnings > 0 {
                std::process::exit(1);
            }
            Ok(())
        }
        Command::Os {
            scheme,
            config,
            output,
        } => {
            let text = std::fs::read_to_string(&config)
                .with_context(|| format!("reading {}", config.display()))?;
            let mut result = transform::apply_os_swap(&text)?;
            let label = match scheme {
                OsScheme::Mac => "macOS",
                OsScheme::Pc => "PC",
            };
            result.insert_str(
                0,
                &format!(
                    "# {label} variant generated by `moergo-layout os` from {}.\n\
                     # Ctrl and GUI are swapped in every binding; regenerate rather than edit.\n",
                    config.display()
                ),
            );
            emit(result, output)
        }
        Command::Alpha {
            layout,
            config,
            output,
            layers,
        } => {
            let text = std::fs::read_to_string(&config)
                .with_context(|| format!("reading {}", config.display()))?;
            let mut result = transform::apply_alpha(&text, layout.as_str(), layers.as_deref())?;
            result.insert_str(
                0,
                &format!(
                    "# {} variant generated by `moergo-layout alpha` from {}.\n\
                     # Regenerate rather than edit.\n",
                    layout.as_str(),
                    config.display()
                ),
            );
            emit(result, output)
        }
        Command::Preset { command } => match command {
            PresetCommand::Apply {
                preset,
                config,
                output,
            } => {
                let preset_text = std::fs::read_to_string(&preset)
                    .with_context(|| format!("reading {}", preset.display()))?;
                let config_text = std::fs::read_to_string(&config)
                    .with_context(|| format!("reading {}", config.display()))?;
                let (result, notes) = preset::apply_preset(&preset_text, &config_text)?;
                for note in &notes {
                    eprintln!("{note}");
                }
                let merged = model::parse(&result).context("re-parsing merged config")?;
                let report = usage::analyze(&merged);
                if report.warnings.is_empty() {
                    eprintln!("merged config analyzes cleanly");
                } else {
                    eprintln!("warnings in the merged config:");
                    for warning in &report.warnings {
                        eprintln!("  - {warning}");
                    }
                }
                emit(result, output)
            }
            PresetCommand::Show { preset } => {
                let preset_text = std::fs::read_to_string(&preset)
                    .with_context(|| format!("reading {}", preset.display()))?;
                print!("{}", preset::describe(&preset_text)?);
                Ok(())
            }
        },
    }
}

fn emit(result: String, output: Option<PathBuf>) -> Result<()> {
    match output {
        Some(path) => {
            std::fs::write(&path, result).with_context(|| format!("writing {}", path.display()))?;
            eprintln!("wrote {}", path.display());
            Ok(())
        }
        None => {
            print!("{result}");
            Ok(())
        }
    }
}
