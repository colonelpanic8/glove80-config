//! Read-only model of a MoErgo RMK runtime configuration TOML, plus the
//! action-token grammar shared by keymap grids, morse fields, combos, forks,
//! and macro operations.
//!
//! The grammar is the QMK-style name set used by `moergo-control keymap`:
//! plain keycodes (`KC_A`, `QK_BOOT`), holes (`--`), transparency (`_______`,
//! `KC_TRNS`), and calls whose arguments may contain spaces and nest:
//! `MO(1)`, `LT(4, KC_BSPC, thumb_layer)`, `MT(KC_A, LGui, hrm_pinky)`,
//! `TH(KC_Q, LSFT(KC_Q), autoshift)`, `LMT(4, MOD_LALT, KC_TAB)`,
//! `MOD(MOD_LCTL|MOD_LALT)`, `TD(0)`, `MACRO(3)`, `USER(2)`.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token(pub String);

impl Token {
    pub fn is_hole(&self) -> bool {
        self.0 == "--"
    }

    pub fn is_transparent(&self) -> bool {
        self.0.chars().all(|c| c == '_') || self.0 == "KC_TRNS"
    }

    pub fn is_bound(&self) -> bool {
        !self.is_hole() && !self.is_transparent() && self.0 != "KC_NO"
    }

    /// `NAME(args)` split into the name and top-level comma-separated args.
    pub fn call(&self) -> Option<(&str, Vec<&str>)> {
        let open = self.0.find('(')?;
        if !self.0.ends_with(')') {
            return None;
        }
        let name = &self.0[..open];
        let body = &self.0[open + 1..self.0.len() - 1];
        let mut args = Vec::new();
        let mut depth = 0usize;
        let mut start = 0usize;
        for (i, c) in body.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => {
                    args.push(body[start..i].trim());
                    start = i + 1;
                }
                _ => {}
            }
        }
        let last = body[start..].trim();
        if !last.is_empty() {
            args.push(last);
        }
        Some((name, args))
    }

    /// Layers this action references, searching nested calls.
    pub fn layer_refs(&self) -> Vec<(String, usize)> {
        let mut refs = Vec::new();
        collect_layer_refs(&self.0, &mut refs);
        refs
    }

    fn indexed_ref(&self, func: &str) -> Option<usize> {
        let (name, args) = self.call()?;
        if name == func {
            args.first()?.parse().ok()
        } else {
            None
        }
    }

    pub fn morse_ref(&self) -> Option<usize> {
        self.indexed_ref("TD")
    }

    pub fn macro_ref(&self) -> Option<usize> {
        self.indexed_ref("MACRO")
    }

    pub fn user_ref(&self) -> Option<usize> {
        self.indexed_ref("USER")
    }

    /// The morse profile named by a typed `MT`/`LT`/`TH` key, if any.
    pub fn profile_ref(&self) -> Option<&str> {
        let (name, args) = self.call()?;
        let arg = match name {
            "MT" | "TH" => *args.get(2)?,
            "LT" => *args.get(2)?,
            _ => return None,
        };
        let bare = arg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        let keycode_like = arg.starts_with("KC_")
            || arg.starts_with("MOD_")
            || arg.chars().next().is_some_and(|c| c.is_ascii_uppercase());
        if bare && !keycode_like {
            Some(arg)
        } else {
            None
        }
    }
}

fn collect_layer_refs(raw: &str, out: &mut Vec<(String, usize)>) {
    let token = Token(raw.to_string());
    if let Some((name, args)) = token.call() {
        if matches!(
            name,
            "MO" | "TG" | "TO" | "TT" | "DF" | "PDF" | "LT" | "LMT"
        ) && let Some(layer) = args.first().and_then(|a| a.parse().ok())
        {
            out.push((name.to_string(), layer));
        }
        for arg in args {
            if arg.contains('(') {
                collect_layer_refs(arg, out);
            }
        }
    }
}

/// Split one grid line into tokens at depth-0 whitespace; `#` starts a
/// comment when outside parentheses.
pub fn split_tokens(line: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for c in line.chars() {
        match c {
            '#' if depth == 0 => break,
            '(' => {
                depth += 1;
                current.push(c);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(c);
            }
            c if c.is_whitespace() && depth == 0 => {
                if !current.is_empty() {
                    tokens.push(Token(std::mem::take(&mut current)));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(Token(current));
    }
    tokens
}

#[derive(Debug)]
pub struct Cell {
    pub row: usize,
    pub col: usize,
    pub token: Token,
}

#[derive(Debug)]
pub struct Layer {
    pub index: usize,
    pub name: String,
    pub cells: Vec<Cell>,
    /// Action strings from `[[layer.bind]]` overrides.
    pub binds: Vec<Token>,
}

#[derive(Debug)]
pub struct Morse {
    pub index: usize,
    pub name: String,
    /// (field, action) pairs for tap/hold/double_tap/hold_after_tap.
    pub actions: Vec<(String, Token)>,
}

#[derive(Debug)]
pub struct Combo {
    pub name: String,
    pub keys: Vec<Token>,
    pub output: Option<Token>,
    pub layer: Option<usize>,
}

#[derive(Debug)]
pub struct Macro {
    pub index: usize,
    pub name: String,
    pub operation_count: usize,
}

#[derive(Debug)]
pub struct Fork {
    pub name: String,
    pub trigger: Option<Token>,
    pub output: Option<Token>,
}

#[derive(Debug)]
pub struct Config {
    pub default_layer: usize,
    pub layers: Vec<Layer>,
    pub morses: Vec<Morse>,
    pub combos: Vec<Combo>,
    pub macros: Vec<Macro>,
    pub forks: Vec<Fork>,
    /// Named `[behavior.morse.profiles.*]` entries.
    pub profiles: Vec<String>,
    /// Durable + conditional scene cell counts per layer.
    pub scene_cells: BTreeMap<usize, usize>,
    pub wake_layers: Vec<usize>,
}

pub fn load(path: &Path) -> Result<Config> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse(&text).with_context(|| format!("parsing {}", path.display()))
}

pub fn parse(text: &str) -> Result<Config> {
    let value: toml::Value = toml::from_str(text)?;
    let root = value
        .as_table()
        .context("runtime config root is not a table")?;
    if root.contains_key("matrix") || root.contains_key("keyboard") {
        bail!(
            "this looks like a compiled keyboard.toml, not a runtime config; \
             pass config/glove80.toml-style files"
        );
    }

    let default_layer = root
        .get("default_layer")
        .and_then(|v| v.as_integer())
        .unwrap_or(0) as usize;

    let mut layers = Vec::new();
    for (index, entry) in table_array(root, "layer").iter().enumerate() {
        let mut cells = Vec::new();
        if let Some(keys) = entry.get("keys").and_then(|v| v.as_str()) {
            for (row, line) in keys
                .lines()
                .filter(|l| !split_tokens(l).is_empty())
                .enumerate()
            {
                for (col, token) in split_tokens(line).into_iter().enumerate() {
                    cells.push(Cell { row, col, token });
                }
            }
        }
        let binds = table_array(entry, "bind")
            .iter()
            .filter_map(|b| b.get("action").and_then(|v| v.as_str()))
            .map(|s| Token(s.to_string()))
            .collect();
        layers.push(Layer {
            index,
            name: entry_name(entry, index, "layer"),
            cells,
            binds,
        });
    }

    let morses = table_array(root, "morse")
        .iter()
        .enumerate()
        .map(|(index, entry)| Morse {
            index,
            name: entry_name(entry, index, "morse"),
            actions: ["tap", "hold", "double_tap", "hold_after_tap"]
                .iter()
                .filter_map(|f| {
                    entry
                        .get(*f)
                        .and_then(|v| v.as_str())
                        .map(|s| (f.to_string(), Token(s.to_string())))
                })
                .collect(),
        })
        .collect();

    let combos = table_array(root, "combo")
        .iter()
        .enumerate()
        .map(|(index, entry)| Combo {
            name: entry_name(entry, index, "combo"),
            keys: entry
                .get("keys")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| Token(s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            output: entry
                .get("output")
                .and_then(|v| v.as_str())
                .map(|s| Token(s.to_string())),
            layer: entry
                .get("layer")
                .and_then(|v| v.as_integer())
                .map(|l| l as usize),
        })
        .collect();

    let macros = table_array(root, "macro")
        .iter()
        .enumerate()
        .map(|(index, entry)| Macro {
            index,
            name: entry_name(entry, index, "macro"),
            operation_count: table_array(entry, "operations").len(),
        })
        .collect();

    let forks = table_array(root, "fork")
        .iter()
        .enumerate()
        .map(|(index, entry)| Fork {
            name: entry_name(entry, index, "fork"),
            trigger: entry
                .get("trigger")
                .and_then(|v| v.as_str())
                .map(|s| Token(s.to_string())),
            output: entry
                .get("output")
                .and_then(|v| v.as_str())
                .map(|s| Token(s.to_string())),
        })
        .collect();

    let profiles = root
        .get("behavior")
        .and_then(|b| b.get("morse"))
        .and_then(|m| m.get("profiles"))
        .and_then(|p| p.as_table())
        .map(|t| t.keys().cloned().collect())
        .unwrap_or_default();

    let lighting = root.get("lighting").and_then(|v| v.as_table());
    let mut scene_cells: BTreeMap<usize, usize> = BTreeMap::new();
    if let Some(lighting) = lighting {
        for entry in table_array_value(lighting.get("scene")) {
            if let Some(layer) = entry.get("layer").and_then(|v| v.as_integer()) {
                *scene_cells.entry(layer as usize).or_default() += 1;
            }
        }
        for entry in table_array_value(lighting.get("conditional_scene")) {
            if let Some(layer) = entry
                .get("layer")
                .and_then(|v| v.get("layer"))
                .and_then(|v| v.as_integer())
            {
                *scene_cells.entry(layer as usize).or_default() += 1;
            }
        }
    }
    let wake_layers = lighting
        .and_then(|l| l.get("wake_layers"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_integer())
                .map(|l| l as usize)
                .collect()
        })
        .unwrap_or_default();

    Ok(Config {
        default_layer,
        layers,
        morses,
        combos,
        macros,
        forks,
        profiles,
        scene_cells,
        wake_layers,
    })
}

fn table_array<'a>(table: &'a toml::value::Table, key: &str) -> Vec<&'a toml::value::Table> {
    table_array_value(table.get(key))
}

fn table_array_value(value: Option<&toml::Value>) -> Vec<&toml::value::Table> {
    value
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_table()).collect())
        .unwrap_or_default()
}

fn entry_name(entry: &toml::value::Table, index: usize, kind: &str) -> String {
    entry
        .get("name")
        .or_else(|| entry.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{kind} {index}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_calls_with_spaces() {
        let tokens =
            split_tokens("KC_A MT(KC_S, LAlt, hrm_ring) LT(4, KC_BSPC, thumb_layer) -- # x y");
        let raw: Vec<&str> = tokens.iter().map(|t| t.0.as_str()).collect();
        assert_eq!(
            raw,
            [
                "KC_A",
                "MT(KC_S, LAlt, hrm_ring)",
                "LT(4, KC_BSPC, thumb_layer)",
                "--"
            ]
        );
    }

    #[test]
    fn extracts_layer_refs_including_nested() {
        assert_eq!(
            Token("LT(1,KC_ESC)".into()).layer_refs(),
            vec![("LT".to_string(), 1)]
        );
        assert_eq!(
            Token("LMT(4, MOD_LALT, KC_TAB)".into()).layer_refs(),
            vec![("LMT".to_string(), 4)]
        );
        assert!(Token("LSFT(KC_9)".into()).layer_refs().is_empty());
    }

    #[test]
    fn extracts_behavior_refs() {
        assert_eq!(Token("TD(0)".into()).morse_ref(), Some(0));
        assert_eq!(Token("MACRO(3)".into()).macro_ref(), Some(3));
        assert_eq!(Token("USER(12)".into()).user_ref(), Some(12));
        assert_eq!(
            Token("MT(KC_A, LGui, hrm_pinky)".into()).profile_ref(),
            Some("hrm_pinky")
        );
        assert_eq!(
            Token("TH(KC_Q, LSFT(KC_Q), autoshift)".into()).profile_ref(),
            Some("autoshift")
        );
        assert_eq!(Token("LT(1,KC_ESC)".into()).profile_ref(), None);
        assert_eq!(Token("LMT(4, MOD_LALT, KC_TAB)".into()).profile_ref(), None);
    }

    #[test]
    fn transparency_and_holes() {
        assert!(Token("_______".into()).is_transparent());
        assert!(Token("KC_TRNS".into()).is_transparent());
        assert!(Token("--".into()).is_hole());
        assert!(Token("KC_A".into()).is_bound());
        assert!(!Token("KC_NO".into()).is_bound());
    }
}
