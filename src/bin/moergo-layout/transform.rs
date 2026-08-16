//! Comment-preserving transforms over a runtime config TOML: the dual-OS
//! Ctrl↔GUI swap and QWERTY→alternate alpha-layout remaps. Both rewrite
//! identifiers inside action strings only; everything else in the document
//! (comments, ordering, unrelated values) is left byte-for-byte intact via
//! `toml_edit`.

use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use toml_edit::{DocumentMut, Item, Value};

use crate::model::split_tokens;

/// Swap Ctrl and GUI everywhere an action can name them: bare keycodes,
/// wrapper functions, `MOD()` masks, and typed-morse modifier names.
/// The mapping is its own inverse, so it converts a PC-canonical config to
/// its macOS variant and vice versa.
pub fn os_swap_map() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("KC_LCTL", "KC_LGUI"),
        ("KC_LGUI", "KC_LCTL"),
        ("KC_RCTL", "KC_RGUI"),
        ("KC_RGUI", "KC_RCTL"),
        ("MOD_LCTL", "MOD_LGUI"),
        ("MOD_LGUI", "MOD_LCTL"),
        ("MOD_RCTL", "MOD_RGUI"),
        ("MOD_RGUI", "MOD_RCTL"),
        ("LCTL", "LGUI"),
        ("LGUI", "LCTL"),
        ("RCTL", "RGUI"),
        ("RGUI", "RCTL"),
        ("LCtrl", "LGui"),
        ("LGui", "LCtrl"),
        ("RCtrl", "RGui"),
        ("RGui", "RCtrl"),
    ])
}

/// QWERTY-position → target-layout identifier maps. The source config must
/// be QWERTY; keys not in the map (modifiers, thumbs, F-row) pass through.
pub fn alpha_map(layout: &str) -> Result<HashMap<&'static str, &'static str>> {
    const QWERTY: &str = "qwertyuiop asdfghjkl;' zxcvbnm,./ -=[]";
    let target = match layout {
        "colemak" => "qwfpgjluy; arstdhneio' zxcvbkm,./ -=[]",
        "colemak-dh" => "qwfpbjluy; arstgmneio' zxcdvkh,./ -=[]",
        "dvorak" => "',.pyfgcrl aoeuidhtns- ;qjkxbmwvz []/=",
        other => bail!("unknown layout \"{other}\" (colemak, colemak-dh, dvorak)"),
    };
    let mut map = HashMap::new();
    for (from, to) in QWERTY.chars().zip(target.chars()) {
        if from == ' ' || from == to {
            continue;
        }
        map.insert(keycode_name(from)?, keycode_name(to)?);
    }
    Ok(map)
}

fn keycode_name(c: char) -> Result<&'static str> {
    Ok(match c {
        'a' => "KC_A",
        'b' => "KC_B",
        'c' => "KC_C",
        'd' => "KC_D",
        'e' => "KC_E",
        'f' => "KC_F",
        'g' => "KC_G",
        'h' => "KC_H",
        'i' => "KC_I",
        'j' => "KC_J",
        'k' => "KC_K",
        'l' => "KC_L",
        'm' => "KC_M",
        'n' => "KC_N",
        'o' => "KC_O",
        'p' => "KC_P",
        'q' => "KC_Q",
        'r' => "KC_R",
        's' => "KC_S",
        't' => "KC_T",
        'u' => "KC_U",
        'v' => "KC_V",
        'w' => "KC_W",
        'x' => "KC_X",
        'y' => "KC_Y",
        'z' => "KC_Z",
        ';' => "KC_SCLN",
        '\'' => "KC_QUOT",
        ',' => "KC_COMM",
        '.' => "KC_DOT",
        '/' => "KC_SLSH",
        '-' => "KC_MINS",
        '=' => "KC_EQL",
        '[' => "KC_LBRC",
        ']' => "KC_RBRC",
        other => bail!("no keycode for '{other}'"),
    })
}

/// Replace identifier runs (`[A-Za-z0-9_]+`) that match the map exactly.
pub fn map_identifiers(raw: &str, map: &HashMap<&str, &str>) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut current = String::new();
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            current.push(c);
        } else {
            flush(&mut out, &mut current, map);
            out.push(c);
        }
    }
    flush(&mut out, &mut current, map);
    out
}

fn flush(out: &mut String, current: &mut String, map: &HashMap<&str, &str>) {
    if current.is_empty() {
        return;
    }
    match map.get(current.as_str()) {
        Some(mapped) => out.push_str(mapped),
        None => out.push_str(current),
    }
    current.clear();
}

/// Rewrite a `keys` grid, mapping identifiers token-wise and re-aligning
/// columns. Token-less lines (blank or comment-only) are preserved verbatim;
/// every content line becomes a column-aligned row of the mapped tokens.
fn map_grid(keys: &str, map: &HashMap<&str, &str>) -> String {
    let rows: Vec<(&str, Option<Vec<String>>)> = keys
        .lines()
        .map(|line| {
            let tokens = split_tokens(line);
            let mapped = if tokens.is_empty() {
                None
            } else {
                Some(
                    tokens
                        .into_iter()
                        .map(|t| map_identifiers(&t.0, map))
                        .collect::<Vec<String>>(),
                )
            };
            (line, mapped)
        })
        .collect();
    align_rows(rows)
}

/// Emit a grid from (original line, replacement tokens) rows: token-less
/// lines verbatim, content lines as column-aligned token rows.
pub(crate) fn align_rows(rows: Vec<(&str, Option<Vec<String>>)>) -> String {
    let mut widths: Vec<usize> = Vec::new();
    for tokens in rows.iter().filter_map(|(_, t)| t.as_ref()) {
        for (col, token) in tokens.iter().enumerate() {
            if widths.len() <= col {
                widths.resize(col + 1, 0);
            }
            widths[col] = widths[col].max(token.len());
        }
    }

    let mut out = String::new();
    for (line, tokens) in rows {
        match tokens {
            None => out.push_str(line),
            Some(tokens) => {
                let last = tokens.len() - 1;
                for (col, token) in tokens.iter().enumerate() {
                    if col < last {
                        out.push_str(&format!(
                            "{:<width$}  ",
                            token,
                            width = widths.get(col).copied().unwrap_or(0)
                        ));
                    } else {
                        out.push_str(token);
                    }
                }
            }
        }
        out.push('\n');
    }
    out
}

/// Apply the OS swap to every action-bearing string in the document:
/// all layer grids and binds, morse actions, combos, forks, and macro
/// operation keycodes.
pub fn apply_os_swap(text: &str) -> Result<String> {
    let map = os_swap_map();
    let mut doc: DocumentMut = text.parse().context("parsing config TOML")?;

    for layer in array_of_tables(&mut doc, "layer") {
        rewrite_grid(layer, &map);
        if let Some(binds) = layer.get_mut("bind").and_then(as_array_of_tables_mut) {
            for bind in binds.iter_mut() {
                rewrite_string(bind, "action", &map);
            }
        }
    }
    for morse in array_of_tables(&mut doc, "morse") {
        for field in ["tap", "hold", "double_tap", "hold_after_tap"] {
            rewrite_string(morse, field, &map);
        }
    }
    for combo in array_of_tables(&mut doc, "combo") {
        rewrite_string(combo, "output", &map);
        if let Some(Item::Value(Value::Array(keys))) = combo.get_mut("keys") {
            for key in keys.iter_mut() {
                if let Value::String(s) = key {
                    let mapped = map_identifiers(s.value(), &map);
                    *key = string_preserving_decor(key, mapped);
                }
            }
        }
    }
    for fork in array_of_tables(&mut doc, "fork") {
        rewrite_string(fork, "trigger", &map);
        rewrite_string(fork, "output", &map);
    }
    for mac in array_of_tables(&mut doc, "macro") {
        if let Some(ops) = mac.get_mut("operations").and_then(as_array_of_tables_mut) {
            for op in ops.iter_mut() {
                rewrite_string(op, "keycode", &map);
            }
        }
    }

    Ok(doc.to_string())
}

/// Remap the alpha layout of the given layers (defaulting to the config's
/// default layer). Other layers — symbol layers, positional layers like
/// Games — are left alone.
pub fn apply_alpha(text: &str, layout: &str, layers: Option<&[usize]>) -> Result<String> {
    let map = alpha_map(layout)?;
    let mut doc: DocumentMut = text.parse().context("parsing config TOML")?;
    let default_layer = doc
        .get("default_layer")
        .and_then(|v| v.as_integer())
        .unwrap_or(0) as usize;
    let selected: Vec<usize> = layers
        .map(|l| l.to_vec())
        .unwrap_or_else(|| vec![default_layer]);

    let all = array_of_tables(&mut doc, "layer");
    if let Some(&bad) = selected.iter().find(|&&l| l >= all.len()) {
        bail!("layer {bad} is out of range ({} layers defined)", all.len());
    }
    for (index, layer) in all.into_iter().enumerate() {
        if selected.contains(&index) {
            rewrite_grid(layer, &map);
        }
    }
    Ok(doc.to_string())
}

pub(crate) fn rewrite_grid(layer: &mut toml_edit::Table, map: &HashMap<&str, &str>) {
    if let Some(item) = layer.get_mut("keys")
        && let Some(keys) = item.as_str()
    {
        let mapped = map_grid(keys, map);
        *item = Item::Value(Value::String(toml_edit::Formatted::new(mapped)));
        if let Some(value) = item.as_value_mut() {
            value.decor_mut().set_prefix(" ");
        }
    }
}

fn rewrite_string(table: &mut toml_edit::Table, key: &str, map: &HashMap<&str, &str>) {
    if let Some(item) = table.get_mut(key)
        && let Some(current) = item.as_str()
    {
        let mapped = map_identifiers(current, map);
        if mapped != current
            && let Some(value) = item.as_value_mut()
        {
            *value = string_preserving_decor(value, mapped);
        }
    }
}

pub(crate) fn string_preserving_decor(original: &Value, new_text: String) -> Value {
    let mut new_value = Value::String(toml_edit::Formatted::new(new_text));
    let decor = original.decor();
    if let Some(prefix) = decor.prefix() {
        new_value.decor_mut().set_prefix(prefix.clone());
    }
    if let Some(suffix) = decor.suffix() {
        new_value.decor_mut().set_suffix(suffix.clone());
    }
    new_value
}

fn as_array_of_tables_mut(item: &mut Item) -> Option<&mut toml_edit::ArrayOfTables> {
    item.as_array_of_tables_mut()
}

fn array_of_tables<'a>(doc: &'a mut DocumentMut, key: &str) -> Vec<&'a mut toml_edit::Table> {
    match doc.get_mut(key) {
        Some(Item::ArrayOfTables(tables)) => tables.iter_mut().collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_swap_is_an_involution() {
        let map = os_swap_map();
        let cases = [
            "KC_LCTL",
            "MOD(MOD_LCTL|MOD_LALT|MOD_LGUI)",
            "LCTL(KC_C)",
            "MT(KC_A, LGui, hrm_pinky)",
            "LMT(4, MOD_LCTL, KC_TAB)",
        ];
        for case in cases {
            let once = map_identifiers(case, &map);
            assert_ne!(once, case, "{case} should change");
            assert_eq!(map_identifiers(&once, &map), case, "{case} round-trip");
        }
        assert_eq!(map_identifiers("KC_LSFT", &map), "KC_LSFT");
    }

    #[test]
    fn identifier_boundaries_are_respected() {
        let map = HashMap::from([("KC_A", "KC_B")]);
        assert_eq!(
            map_identifiers("KC_AT KC_A LSFT(KC_A)", &map),
            "KC_AT KC_B LSFT(KC_B)"
        );
    }

    #[test]
    fn colemak_maps_home_row() {
        let map = alpha_map("colemak").unwrap();
        assert_eq!(map.get("KC_S"), Some(&"KC_R"));
        assert_eq!(map.get("KC_SCLN"), Some(&"KC_O"));
        assert_eq!(map.get("KC_P"), Some(&"KC_SCLN"));
        assert!(!map.contains_key("KC_A"));
        assert!(!map.contains_key("KC_QUOT"));
    }

    #[test]
    fn dvorak_maps_punctuation() {
        let map = alpha_map("dvorak").unwrap();
        assert_eq!(map.get("KC_Q"), Some(&"KC_QUOT"));
        assert_eq!(map.get("KC_MINS"), Some(&"KC_LBRC"));
        assert_eq!(map.get("KC_LBRC"), Some(&"KC_SLSH"));
        assert_eq!(map.get("KC_QUOT"), Some(&"KC_MINS"));
    }

    const FIXTURE: &str = r#"# a leading comment that must survive
default_layer = 0

# layer comment
[[layer]]
name = "Base"
keys = """
KC_TAB   KC_Q  KC_LCTL
MO(1)    KC_S  LT(1,KC_SCLN)
"""

[[layer]]
name = "Games"
keys = """
KC_W  KC_A  KC_LCTL
--    --    _______
"""

[[morse]]
name = "m"
tap = "KC_DEL"
hold = "LCTL(KC_C)"

[[combo]]
name = "c"
keys = ["KC_LCTL", "KC_Q"]
output = "LMT(4, MOD_LCTL, KC_TAB)"
"#;

    #[test]
    fn os_swap_touches_all_action_sites_and_keeps_comments() {
        let swapped = apply_os_swap(FIXTURE).unwrap();
        assert!(swapped.contains("# a leading comment that must survive"));
        assert!(swapped.contains("# layer comment"));
        assert!(swapped.contains("hold = \"LGUI(KC_C)\""));
        assert!(swapped.contains("keys = [\"KC_LGUI\", \"KC_Q\"]"));
        assert!(swapped.contains("output = \"LMT(4, MOD_LGUI, KC_TAB)\""));
        assert!(!swapped.contains("KC_LCTL"));
        let round_trip = apply_os_swap(&swapped).unwrap();
        let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(normalize(&round_trip), normalize(FIXTURE));
    }

    #[test]
    fn alpha_only_touches_selected_layers() {
        let remapped = apply_alpha(FIXTURE, "colemak", None).unwrap();
        // Base layer: KC_S -> KC_R, LT tap KC_SCLN -> KC_O.
        assert!(remapped.contains("KC_R"));
        assert!(remapped.contains("LT(1,KC_O)"));
        // Games layer untouched: still WASD.
        let games = remapped.split("Games").nth(1).unwrap();
        assert!(games.contains("KC_W  KC_A"));
        // Modifiers pass through.
        assert!(remapped.contains("KC_LCTL"));
    }

    #[test]
    fn grid_realignment_keeps_row_shape() {
        let map = alpha_map("colemak").unwrap();
        let grid = map_grid("\nKC_Q KC_W\n\nKC_S KC_TRNS\n", &map);
        assert_eq!(grid, "\nKC_Q  KC_W\n\nKC_R  KC_TRNS\n");
    }
}
