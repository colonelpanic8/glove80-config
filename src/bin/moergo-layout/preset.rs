//! Partial preset (fragment) application. A preset TOML carries a portion
//! of a layout — new layers, morse entries, combos, macros, forks, morse
//! profiles, lighting scene cells, and sparse patches to existing keys —
//! and `preset apply` merges it into a runtime config.
//!
//! Slots are resolved at apply time: fragment-defined layers, morses, and
//! macros get the next free indices in the target, and `$name` references
//! inside action strings (`MO($symbols)`, `TD($sym_hold)`) are rewritten
//! to those indices. `layer` fields of combos, scenes, and patches accept
//! an integer (absolute target slot), `"$id"` (fragment layer), `"default"`
//! (the target's default layer), or a target layer id/name. Inside a patch
//! action, `$key` stands for the key currently bound at that cell.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result, bail};
use toml_edit::{ArrayOfTables, DocumentMut, Formatted, Item, Table, Value};

use crate::address::{self, Board};
use crate::model::{self, Token, split_tokens};
use crate::transform;

pub fn apply_preset(preset_text: &str, config_text: &str) -> Result<(String, Vec<String>)> {
    let preset_doc: DocumentMut = preset_text.parse().context("parsing preset TOML")?;
    let mut doc: DocumentMut = config_text.parse().context("parsing config TOML")?;
    let target = model::parse(config_text)?;
    check_sections(&preset_doc)?;

    let header = header(&preset_doc)?;
    let name = preset_name(header)?;
    let mut notes = vec![format!("preset \"{name}\"")];

    if let Some(geometry) = header.get("geometry") {
        check_geometry(geometry, &target)?;
    }
    let board = Board::detect(&target);
    if let Some(board) = board {
        notes.push(format!("  target board: {}", board.name()));
    }
    if let Some(boards) = header.get("boards") {
        check_boards(boards, board)?;
    }

    let frag_layers = clone_tables(preset_doc.get("layer"));
    let frag_morses = clone_tables(preset_doc.get("morse"));
    let frag_combos = clone_tables(preset_doc.get("combo"));
    let frag_macros = clone_tables(preset_doc.get("macro"));
    let frag_forks = clone_tables(preset_doc.get("fork"));
    let frag_scenes = clone_tables(preset_doc.get("lighting").and_then(|l| l.get("scene")));
    let frag_patches = clone_tables(preset_doc.get("patch"));
    let frag_profiles = profile_tables(&preset_doc);

    let symbols = build_symbols(
        &target,
        &doc,
        &frag_layers,
        &frag_morses,
        &frag_macros,
        &frag_combos,
        &frag_forks,
        &mut notes,
    )?;

    let mut frag_layers = frag_layers;
    let mut frag_morses = frag_morses;
    let mut frag_combos = frag_combos;
    let mut frag_macros = frag_macros;
    let mut frag_forks = frag_forks;
    let mut frag_scenes = frag_scenes;
    for layer in frag_layers.iter_mut() {
        resolve_layer_keys(layer, board)?;
    }
    for table in frag_layers
        .iter_mut()
        .chain(frag_morses.iter_mut())
        .chain(frag_combos.iter_mut())
        .chain(frag_macros.iter_mut())
        .chain(frag_forks.iter_mut())
        .chain(frag_scenes.iter_mut())
    {
        substitute_table(table, &symbols)?;
    }
    for layer in frag_layers.iter_mut() {
        transform::rewrite_grid(layer, &HashMap::new());
    }
    for table in frag_combos.iter_mut().chain(frag_scenes.iter_mut()) {
        resolve_layer_field(table, &doc, target.default_layer)?;
    }
    for table in frag_scenes.iter_mut() {
        resolve_key_field(table, board)?;
    }
    // Scene cells extend [lighting], whose schema requires the full section;
    // appending them to a config without one would render an invalid file.
    if !frag_scenes.is_empty()
        && doc
            .get("lighting")
            .and_then(|l| l.get("brightness"))
            .is_none()
    {
        bail!(
            "this preset carries lighting scene cells, but the target has no [lighting] \
             section; pull or add one first"
        );
    }

    for patch in &frag_patches {
        apply_patch(
            &mut doc,
            patch,
            &symbols,
            board,
            target.default_layer,
            &mut notes,
        )?;
    }

    for (profile_name, profile) in frag_profiles {
        merge_profile(&mut doc, &profile_name, profile, &mut notes)?;
    }

    let counts = [
        ("combo", frag_combos.len()),
        ("macro", frag_macros.len()),
        ("fork", frag_forks.len()),
        ("lighting scene", frag_scenes.len()),
    ];
    for (kind, count) in counts {
        if count > 0 {
            notes.push(format!("  {count} {kind} entr{}", plural_y(count)));
        }
    }

    append_tables(&mut doc, &["layer"], frag_layers)?;
    append_tables(&mut doc, &["morse"], frag_morses)?;
    append_tables(&mut doc, &["combo"], frag_combos)?;
    append_tables(&mut doc, &["macro"], frag_macros)?;
    append_tables(&mut doc, &["fork"], frag_forks)?;
    append_tables(&mut doc, &["lighting", "scene"], frag_scenes)?;

    // Tables cloned from the preset document keep their *source* positions,
    // which would interleave them into the middle of the target. Renumber
    // every table in structure order so appended entries land at the end of
    // their own groups.
    let mut next_position = 0;
    renumber_positions(doc.as_table_mut(), &mut next_position);

    Ok((doc.to_string(), notes))
}

/// Structural summary of a preset file, without a target config.
pub fn describe(preset_text: &str) -> Result<String> {
    let preset_doc: DocumentMut = preset_text.parse().context("parsing preset TOML")?;
    check_sections(&preset_doc)?;
    let header = header(&preset_doc)?;
    let name = preset_name(header)?;

    let mut out = format!("preset \"{name}\"");
    if let Some(description) = header.get("description").and_then(|v| v.as_str()) {
        out.push_str(&format!(" — {description}"));
    }
    out.push('\n');
    if let Some(geometry) = header.get("geometry").and_then(|g| g.as_array()) {
        let dims: Vec<String> = geometry
            .iter()
            .map(|v| v.to_string().trim().into())
            .collect();
        out.push_str(&format!("geometry: {}\n", dims.join("x")));
    }
    if let Some(boards) = header.get("boards").and_then(|g| g.as_array()) {
        let names: Vec<&str> = boards.iter().filter_map(|v| v.as_str()).collect();
        out.push_str(&format!("boards: {}\n", names.join(", ")));
    }

    for (kind, key, label_key) in [
        ("layer", "layer", "id"),
        ("morse", "morse", "name"),
        ("combo", "combo", "name"),
        ("macro", "macro", "name"),
        ("fork", "fork", "name"),
    ] {
        let tables = clone_tables(preset_doc.get(key));
        if tables.is_empty() {
            continue;
        }
        let labels: Vec<String> = tables
            .iter()
            .map(|t| {
                t.get(label_key)
                    .and_then(|v| v.as_str())
                    .map(|s| format!("${s}"))
                    .unwrap_or_else(|| "(unnamed)".to_string())
            })
            .collect();
        out.push_str(&format!("{kind}s: {}\n", labels.join(", ")));
    }

    let profiles = profile_tables(&preset_doc);
    if !profiles.is_empty() {
        let names: Vec<&str> = profiles.iter().map(|(n, _)| n.as_str()).collect();
        out.push_str(&format!("morse profiles: {}\n", names.join(", ")));
    }

    for patch in clone_tables(preset_doc.get("patch")) {
        let layer = patch
            .get("layer")
            .map(|i| i.to_string().trim().to_string())
            .unwrap_or_else(|| "?".to_string());
        for key in clone_tables(patch.get("key")) {
            let at = key
                .get("at")
                .map(|i| i.to_string().trim().to_string())
                .unwrap_or_else(|| "?".to_string());
            let action = key.get("action").and_then(|v| v.as_str()).unwrap_or("?");
            out.push_str(&format!("patch: layer {layer} at {at} -> {action}\n"));
        }
    }

    let scenes = clone_tables(preset_doc.get("lighting").and_then(|l| l.get("scene")));
    if !scenes.is_empty() {
        out.push_str(&format!("lighting scene cells: {}\n", scenes.len()));
    }
    Ok(out)
}

fn header(preset_doc: &DocumentMut) -> Result<&Table> {
    preset_doc
        .get("preset")
        .and_then(|i| i.as_table())
        .context("preset file must start with a [preset] table")
}

fn preset_name(header: &Table) -> Result<String> {
    Ok(header
        .get("name")
        .and_then(|v| v.as_str())
        .context("[preset] needs a name")?
        .to_string())
}

/// Reject unknown sections so typos fail loudly instead of being dropped.
fn check_sections(preset_doc: &DocumentMut) -> Result<()> {
    const ALLOWED: [&str; 9] = [
        "preset", "layer", "morse", "combo", "macro", "fork", "patch", "behavior", "lighting",
    ];
    for (key, _) in preset_doc.iter() {
        if !ALLOWED.contains(&key) {
            bail!("unsupported preset section [{key}]");
        }
    }
    if let Some(behavior) = preset_doc.get("behavior").and_then(|i| i.as_table()) {
        for (key, item) in behavior.iter() {
            if key != "morse" {
                bail!("presets may only carry [behavior.morse.profiles.*], not [behavior.{key}]");
            }
            if let Some(morse) = item.as_table() {
                for (sub, _) in morse.iter() {
                    if sub != "profiles" {
                        bail!(
                            "presets may only carry [behavior.morse.profiles.*], \
                             not [behavior.morse].{sub}"
                        );
                    }
                }
            }
        }
    }
    if let Some(lighting) = preset_doc.get("lighting").and_then(|i| i.as_table()) {
        for (key, _) in lighting.iter() {
            if key != "scene" {
                bail!("presets may only carry [[lighting.scene]] entries, not [lighting.{key}]");
            }
        }
    }
    Ok(())
}

/// Enforce a `[preset] boards = ["glove80", ...]` declaration against the
/// board detected from the target's grid shape.
fn check_boards(boards: &Item, board: Option<Board>) -> Result<()> {
    let names: Vec<String> = boards
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();
    if names.is_empty() {
        bail!("[preset] boards must be a list of board names (\"glove80\", \"go60\")");
    }
    for name in &names {
        if Board::from_name(name).is_none() {
            bail!("[preset] boards names unknown board \"{name}\"");
        }
    }
    let Some(board) = board else {
        bail!(
            "this preset is for {} but the target grid is not a recognized MoErgo board",
            names.join("/")
        );
    };
    if !names.iter().any(|n| n == board.name()) {
        bail!(
            "this preset supports {} but the target is a {}",
            names.join("/"),
            board.name()
        );
    }
    Ok(())
}

/// A cell position: `[row, col]`, a physical address string (`"LH-C4R4"`),
/// or a per-board table of either (`{ glove80 = "LH-C4R4", go60 = [2, 2] }`).
fn resolve_position(item: &Item, board: Option<Board>, what: &str) -> Result<(usize, usize)> {
    if let Some(value) = item.as_value() {
        return position_from_value(value, board, what);
    }
    if let Some(table) = item.as_table() {
        return position_from_board_entry(
            |name| table.get(name).and_then(|i| i.as_value()),
            board,
            what,
        );
    }
    bail!("{what} must be [row, col], a physical address, or a per-board table");
}

fn position_from_value(value: &Value, board: Option<Board>, what: &str) -> Result<(usize, usize)> {
    match value {
        Value::Array(array) => {
            let dims: Vec<usize> = array
                .iter()
                .filter_map(|v| v.as_integer())
                .map(|v| v as usize)
                .collect();
            let [row, col] = dims[..] else {
                bail!("{what} array form must be [row, col]");
            };
            Ok((row, col))
        }
        Value::String(s) => {
            let Some(board) = board else {
                bail!(
                    "{what} uses physical address \"{}\", but the target grid is not a \
                     recognized MoErgo board (6x14 or 5x14)",
                    s.value()
                );
            };
            address::resolve(s.value(), board)
        }
        Value::InlineTable(table) => position_from_board_entry(|name| table.get(name), board, what),
        _ => bail!("{what} must be [row, col], a physical address, or a per-board table"),
    }
}

fn position_from_board_entry<'a>(
    get: impl Fn(&str) -> Option<&'a Value>,
    board: Option<Board>,
    what: &str,
) -> Result<(usize, usize)> {
    let Some(board) = board else {
        bail!(
            "{what} is per-board, but the target grid is not a recognized MoErgo board \
             (6x14 or 5x14)"
        );
    };
    let Some(entry) = get(board.name()) else {
        bail!("{what} has no entry for board \"{}\"", board.name());
    };
    position_from_value(entry, Some(board), what)
}

/// Pick the detected board's grid when a fragment layer carries per-board
/// `keys.<board>` grids instead of a single `keys` string.
fn resolve_layer_keys(layer: &mut Table, board: Option<Board>) -> Result<()> {
    let Some(item) = layer.get_mut("keys") else {
        return Ok(());
    };
    if item.as_str().is_some() {
        return Ok(());
    }
    let Some(board) = board else {
        bail!(
            "this preset layer has per-board keys, but the target grid is not a \
             recognized MoErgo board (6x14 or 5x14)"
        );
    };
    let text = match &*item {
        Item::Table(table) => table.get(board.name()).and_then(|i| i.as_str()),
        Item::Value(Value::InlineTable(table)) => table.get(board.name()).and_then(|v| v.as_str()),
        _ => None,
    }
    .with_context(|| format!("layer keys has no grid for board \"{}\"", board.name()))?
    .to_string();
    *item = Item::Value(Value::String(Formatted::new(text)));
    if let Some(value) = item.as_value_mut() {
        value.decor_mut().set_prefix(" ");
    }
    Ok(())
}

/// Turn a scene cell's `key` selector into the `[row, col]` array the
/// runtime schema expects, when it uses an address or per-board form.
fn resolve_key_field(table: &mut Table, board: Option<Board>) -> Result<()> {
    let Some(item) = table.get_mut("key") else {
        return Ok(());
    };
    let needs_resolution = match item.as_value() {
        Some(Value::String(_)) | Some(Value::InlineTable(_)) => true,
        _ => item.as_table().is_some(),
    };
    if !needs_resolution {
        return Ok(());
    }
    let (row, col) = resolve_position(item, board, "scene cell key")?;
    let mut array = toml_edit::Array::new();
    array.push(row as i64);
    array.push(col as i64);
    let mut value = Value::Array(array);
    value.decor_mut().set_prefix(" ");
    *item = Item::Value(value);
    Ok(())
}

fn check_geometry(geometry: &Item, target: &model::Config) -> Result<()> {
    let dims: Vec<usize> = geometry
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_integer())
                .map(|v| v as usize)
                .collect()
        })
        .unwrap_or_default();
    let [rows, cols] = dims[..] else {
        bail!("[preset] geometry must be [rows, cols]");
    };
    let Some(default) = target.layers.get(target.default_layer) else {
        return Ok(());
    };
    let have_rows = default.cells.iter().map(|c| c.row + 1).max().unwrap_or(0);
    let have_cols = default.cells.iter().map(|c| c.col + 1).max().unwrap_or(0);
    if (have_rows, have_cols) != (rows, cols) {
        bail!(
            "preset expects a {rows}x{cols} grid but the target's default layer \
             is {have_rows}x{have_cols}"
        );
    }
    Ok(())
}

fn clone_tables(item: Option<&Item>) -> Vec<Table> {
    item.and_then(|i| i.as_array_of_tables())
        .map(|a| a.iter().cloned().collect())
        .unwrap_or_default()
}

fn profile_tables(preset_doc: &DocumentMut) -> Vec<(String, Table)> {
    preset_doc
        .get("behavior")
        .and_then(|b| b.get("morse"))
        .and_then(|m| m.get("profiles"))
        .and_then(|p| p.as_table())
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| v.as_table().map(|table| (k.to_string(), table.clone())))
                .collect()
        })
        .unwrap_or_default()
}

fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with(|c: char| c.is_ascii_digit())
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `(id, name)` labels of every `[[layer]]` in the target document.
fn layer_labels(doc: &DocumentMut) -> Vec<(Option<String>, Option<String>)> {
    clone_tables(doc.get("layer"))
        .iter()
        .map(|t| {
            let get = |key: &str| t.get(key).and_then(|v| v.as_str()).map(|s| s.to_string());
            (get("id"), get("name"))
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn build_symbols(
    target: &model::Config,
    doc: &DocumentMut,
    frag_layers: &[Table],
    frag_morses: &[Table],
    frag_macros: &[Table],
    frag_combos: &[Table],
    frag_forks: &[Table],
    notes: &mut Vec<String>,
) -> Result<HashMap<String, String>> {
    let mut symbols: HashMap<String, String> = HashMap::new();
    let define = |symbols: &mut HashMap<String, String>, sym: &str, value: usize| {
        if sym == "key" {
            bail!("\"key\" is a reserved preset symbol ($key is the patched cell's action)");
        }
        if symbols.insert(sym.to_string(), value.to_string()).is_some() {
            bail!(
                "duplicate preset symbol ${sym}: layer ids and morse/macro names share one namespace"
            );
        }
        Ok(())
    };

    let target_labels = layer_labels(doc);
    for (index, layer) in frag_layers.iter().enumerate() {
        let id = layer
            .get("id")
            .and_then(|v| v.as_str())
            .context("every preset [[layer]] needs an id (its $reference name)")?;
        if !is_ident(id) {
            bail!("preset layer id \"{id}\" must be identifier-like to be referencable");
        }
        if target_labels
            .iter()
            .any(|(tid, tname)| tid.as_deref() == Some(id) || tname.as_deref() == Some(id))
        {
            bail!("target config already has a layer \"{id}\"");
        }
        let slot = target.layers.len() + index;
        define(&mut symbols, id, slot)?;
        notes.push(format!("  layer ${id} -> slot {slot}"));
    }

    for (kind, existing, tables, offset) in [
        (
            "morse",
            target
                .morses
                .iter()
                .map(|m| m.name.clone())
                .collect::<Vec<_>>(),
            frag_morses,
            target.morses.len(),
        ),
        (
            "macro",
            target.macros.iter().map(|m| m.name.clone()).collect(),
            frag_macros,
            target.macros.len(),
        ),
    ] {
        for (index, table) in tables.iter().enumerate() {
            let entry_name = table
                .get("id")
                .or_else(|| table.get("name"))
                .and_then(|v| v.as_str())
                .with_context(|| format!("every preset [[{kind}]] needs a name"))?;
            if existing.iter().any(|n| n == entry_name) {
                bail!("target config already has a {kind} named \"{entry_name}\"");
            }
            let slot = offset + index;
            if is_ident(entry_name) {
                define(&mut symbols, entry_name, slot)?;
            }
            let call = if kind == "morse" { "TD" } else { "MACRO" };
            notes.push(format!("  {kind} \"{entry_name}\" -> {call}({slot})"));
        }
    }

    for (kind, existing, tables) in [
        (
            "combo",
            target
                .combos
                .iter()
                .map(|c| c.name.clone())
                .collect::<Vec<_>>(),
            frag_combos,
        ),
        (
            "fork",
            target.forks.iter().map(|f| f.name.clone()).collect(),
            frag_forks,
        ),
    ] {
        for table in tables {
            if let Some(entry_name) = table.get("name").and_then(|v| v.as_str())
                && existing.iter().any(|n| n == entry_name)
            {
                bail!("target config already has a {kind} named \"{entry_name}\"");
            }
        }
    }

    Ok(symbols)
}

/// Replace `$ident` occurrences using the symbol map; unknown symbols are
/// errors. `$` not followed by an identifier passes through.
fn substitute(raw: &str, symbols: &HashMap<String, String>) -> Result<String> {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        let mut ident = String::new();
        while let Some(&next) = chars.peek() {
            if next.is_ascii_alphanumeric() || next == '_' {
                ident.push(next);
                chars.next();
            } else {
                break;
            }
        }
        if ident.is_empty() {
            out.push('$');
            continue;
        }
        match symbols.get(&ident) {
            Some(value) => out.push_str(value),
            None => bail!("unknown preset reference ${ident}"),
        }
    }
    Ok(out)
}

fn substitute_table(table: &mut Table, symbols: &HashMap<String, String>) -> Result<()> {
    let keys: Vec<String> = table.iter().map(|(k, _)| k.to_string()).collect();
    for key in keys {
        if let Some(item) = table.get_mut(&key) {
            substitute_item(item, symbols)?;
        }
    }
    Ok(())
}

fn substitute_item(item: &mut Item, symbols: &HashMap<String, String>) -> Result<()> {
    match item {
        Item::Value(value) => substitute_value(value, symbols),
        Item::Table(table) => substitute_table(table, symbols),
        Item::ArrayOfTables(tables) => {
            for table in tables.iter_mut() {
                substitute_table(table, symbols)?;
            }
            Ok(())
        }
        Item::None => Ok(()),
    }
}

fn substitute_value(value: &mut Value, symbols: &HashMap<String, String>) -> Result<()> {
    match value {
        Value::String(s) => {
            let mapped = substitute(s.value(), symbols)?;
            if mapped != *s.value() {
                *value = transform::string_preserving_decor(value, mapped);
            }
            Ok(())
        }
        Value::Array(array) => {
            for entry in array.iter_mut() {
                substitute_value(entry, symbols)?;
            }
            Ok(())
        }
        Value::InlineTable(table) => {
            let keys: Vec<String> = table.iter().map(|(k, _)| k.to_string()).collect();
            for key in keys {
                if let Some(entry) = table.get_mut(&key) {
                    substitute_value(entry, symbols)?;
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Turn a string `layer` field ("6" after substitution, "default", or a
/// target layer id/name) into the integer slot the runtime schema expects.
fn resolve_layer_field(table: &mut Table, doc: &DocumentMut, default_layer: usize) -> Result<()> {
    let Some(item) = table.get_mut("layer") else {
        return Ok(());
    };
    let Some(selector) = item.as_str().map(|s| s.to_string()) else {
        return Ok(());
    };
    let index = layer_selector_index(&selector, doc, default_layer)?;
    let mut value = Value::Integer(Formatted::new(index as i64));
    value.decor_mut().set_prefix(" ");
    *item = Item::Value(value);
    Ok(())
}

fn layer_selector_index(selector: &str, doc: &DocumentMut, default_layer: usize) -> Result<usize> {
    if let Ok(index) = selector.parse::<usize>() {
        return Ok(index);
    }
    if selector == "default" {
        return Ok(default_layer);
    }
    let labels = layer_labels(doc);
    let matches: Vec<usize> = labels
        .iter()
        .enumerate()
        .filter(|(_, (id, name))| {
            id.as_deref() == Some(selector) || name.as_deref() == Some(selector)
        })
        .map(|(i, _)| i)
        .collect();
    match matches[..] {
        [index] => Ok(index),
        [] => bail!("no target layer matches \"{selector}\""),
        _ => bail!("layer selector \"{selector}\" is ambiguous in the target config"),
    }
}

fn apply_patch(
    doc: &mut DocumentMut,
    patch: &Table,
    symbols: &HashMap<String, String>,
    board: Option<Board>,
    default_layer: usize,
    notes: &mut Vec<String>,
) -> Result<()> {
    let selector = patch.get("layer").context("[[patch]] needs a layer")?;
    let layer_index = if let Some(index) = selector.as_integer() {
        index as usize
    } else if let Some(raw) = selector.as_str() {
        let substituted = substitute(raw, symbols)?;
        layer_selector_index(&substituted, doc, default_layer)?
    } else {
        bail!("[[patch]] layer must be an integer or a string selector");
    };

    let label = {
        let labels = layer_labels(doc);
        let Some((id, name)) = labels.get(layer_index) else {
            bail!(
                "patch targets layer {layer_index}, but the config defines {} layers",
                labels.len()
            );
        };
        name.clone()
            .or_else(|| id.clone())
            .unwrap_or_else(|| layer_index.to_string())
    };

    let edits: Vec<(usize, usize, String)> = clone_tables(patch.get("key"))
        .iter()
        .map(|key| {
            let at = key
                .get("at")
                .context("[[patch.key]] needs at = [row, col] or a physical address")?;
            let (row, col) = resolve_position(at, board, "[[patch.key]] at")?;
            let action = key
                .get("action")
                .and_then(|v| v.as_str())
                .context("[[patch.key]] needs an action")?;
            Ok((row, col, action.to_string()))
        })
        .collect::<Result<_>>()?;

    let layers = doc
        .get_mut("layer")
        .and_then(|i| i.as_array_of_tables_mut())
        .context("target config has no [[layer]] tables")?;
    let layer = layers
        .get_mut(layer_index)
        .with_context(|| format!("patch targets layer {layer_index}, which is not defined"))?;
    let keys_item = layer
        .get_mut("keys")
        .with_context(|| format!("layer {layer_index} has no keys grid to patch"))?;
    let grid = keys_item
        .as_str()
        .with_context(|| format!("layer {layer_index} keys is not a string"))?
        .to_string();

    let mut rows: Vec<(String, Option<Vec<String>>)> = grid
        .lines()
        .map(|line| {
            let tokens = split_tokens(line);
            let tokens = if tokens.is_empty() {
                None
            } else {
                Some(tokens.into_iter().map(|t| t.0).collect::<Vec<String>>())
            };
            (line.to_string(), tokens)
        })
        .collect();
    let content_rows: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, (_, tokens))| tokens.is_some())
        .map(|(i, _)| i)
        .collect();

    for (row, col, action) in edits {
        let line_index = *content_rows.get(row).with_context(|| {
            format!("patch cell [{row}, {col}] is beyond the grid of layer {layer_index}")
        })?;
        let tokens = rows[line_index].1.as_mut().expect("content row has tokens");
        let existing = tokens
            .get(col)
            .with_context(|| {
                format!("patch cell [{row}, {col}] is beyond the grid of layer {layer_index}")
            })?
            .clone();
        if Token(existing.clone()).is_hole() {
            bail!("patch cell [{row}, {col}] on layer {layer_index} is a physical hole (--)");
        }
        let resolved = if action.contains('$') {
            let mut cell_symbols = symbols.clone();
            if is_ident(&existing) && Token(existing.clone()).is_bound() {
                cell_symbols.insert("key".to_string(), existing.clone());
            } else if substitute(&action, &cell_symbols).is_err() {
                // Only complain about $key when the action actually uses it.
                let mut probe = symbols.clone();
                probe.insert("key".to_string(), "KC_NO".to_string());
                if substitute(&action, &probe).is_ok() {
                    bail!(
                        "patch cell [{row}, {col}] on layer {layer_index} holds \"{existing}\", \
                         not a plain keycode; $key patches need a bare key there"
                    );
                }
            }
            substitute(&action, &cell_symbols)?
        } else {
            action
        };
        notes.push(format!(
            "  patch {label}[{row},{col}]: {existing} -> {resolved}"
        ));
        tokens[col] = resolved;
    }

    let row_refs: Vec<(&str, Option<Vec<String>>)> = rows
        .iter()
        .map(|(line, tokens)| (line.as_str(), tokens.clone()))
        .collect();
    let aligned = transform::align_rows(row_refs);
    *keys_item = Item::Value(Value::String(Formatted::new(aligned)));
    if let Some(value) = keys_item.as_value_mut() {
        value.decor_mut().set_prefix(" ");
    }
    Ok(())
}

fn merge_profile(
    doc: &mut DocumentMut,
    name: &str,
    profile: Table,
    notes: &mut Vec<String>,
) -> Result<()> {
    let existing = doc
        .get("behavior")
        .and_then(|b| b.get("morse"))
        .and_then(|m| m.get("profiles"))
        .and_then(|p| p.get(name))
        .and_then(|i| i.as_table());
    if let Some(existing) = existing {
        if table_fingerprint(existing) == table_fingerprint(&profile) {
            notes.push(format!("  profile {name}: already present, skipped"));
            return Ok(());
        }
        bail!(
            "morse profile \"{name}\" already exists with different settings; \
             rename it in the preset or reconcile the config first"
        );
    }

    let mut current = doc.as_table_mut();
    for key in ["behavior", "morse", "profiles"] {
        let item = current.entry(key).or_insert_with(|| {
            let mut table = Table::new();
            table.set_implicit(true);
            Item::Table(table)
        });
        current = item
            .as_table_mut()
            .with_context(|| format!("[{key}] in the target config is not a table"))?;
    }
    current.insert(name, Item::Table(profile));
    notes.push(format!("  profile {name}: added"));
    Ok(())
}

fn table_fingerprint(table: &Table) -> BTreeMap<String, String> {
    table
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string().trim().to_string()))
        .collect()
}

fn append_tables(doc: &mut DocumentMut, path: &[&str], tables: Vec<Table>) -> Result<()> {
    if tables.is_empty() {
        return Ok(());
    }
    let mut current = doc.as_table_mut();
    let (last, parents) = path.split_last().expect("append path is non-empty");
    for key in parents {
        let item = current.entry(key).or_insert_with(|| {
            let mut table = Table::new();
            table.set_implicit(true);
            Item::Table(table)
        });
        current = item
            .as_table_mut()
            .with_context(|| format!("[{key}] in the target config is not a table"))?;
    }
    let item = current
        .entry(last)
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()));
    let aot = item.as_array_of_tables_mut().with_context(|| {
        format!(
            "{} in the target config is not an array of tables",
            path.join(".")
        )
    })?;
    for table in tables {
        aot.push(table);
    }
    Ok(())
}

fn renumber_positions(table: &mut Table, next: &mut usize) {
    let keys: Vec<String> = table.iter().map(|(k, _)| k.to_string()).collect();
    for key in keys {
        match table.get_mut(&key) {
            Some(Item::Table(sub)) => {
                if !sub.is_implicit() {
                    sub.set_position(*next);
                    *next += 1;
                }
                renumber_positions(sub, next);
            }
            Some(Item::ArrayOfTables(tables)) => {
                for sub in tables.iter_mut() {
                    sub.set_position(*next);
                    *next += 1;
                    renumber_positions(sub, next);
                }
            }
            _ => {}
        }
    }
}

fn plural_y(count: usize) -> &'static str {
    if count == 1 { "y" } else { "ies" }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TARGET: &str = r#"# target header comment that must survive
default_layer = 0

[[layer]]
id = "base"
name = "Base"
keys = """
KC_TAB   KC_Q  TD(0)
KC_LSFT  KC_S  LT(1,KC_SCLN)
"""

[[layer]]
id = "lower"
name = "Lower"
keys = """
_______  _______  TH(KC_A, LSFT(KC_A), autoshift)
--       _______  _______
"""

[[morse]]
name = "Existing"
tap = "KC_DEL"
hold = "MO(1)"

[behavior.morse.profiles.autoshift]
hold_timeout_ms = 190

[lighting]
brightness = 255
"#;

    const PRESET: &str = r##"[preset]
name = "test"
description = "test preset"
geometry = [2, 3]

[behavior.morse.profiles.autoshift]
hold_timeout_ms = 190

[behavior.morse.profiles.hrm]
hold_timeout_ms = 270

[[layer]]
id = "symbols"
name = "Symbols"
keys = """
KC_1  TD($sym_hold)  MO($symbols)
--    _______        _______
"""

[[morse]]
name = "sym_hold"
tap = "KC_2"
hold = "MO($symbols)"

[[patch]]
layer = "default"

[[patch.key]]
at = [0, 1]
action = "MT($key, LGui, hrm)"

[[patch.key]]
at = [0, 0]
action = "MO($symbols)"

[[combo]]
name = "sym_combo"
keys = ["KC_Q", "KC_S"]
output = "TO($symbols)"
layer = "lower"

[[lighting.scene]]
layer = "$symbols"
key = [0, 1]
color = "#0050ff"
effect = "solid"
"##;

    #[test]
    fn applies_fragment_with_symbol_resolution() {
        let (result, notes) = apply_preset(PRESET, TARGET).unwrap();
        // Fragment layer lands in slot 2; fragment morse in TD(1).
        assert!(result.contains("# target header comment that must survive"));
        assert!(result.contains("name = \"Symbols\""));
        assert!(result.contains("TD(1)"));
        assert!(result.contains("MO(2)"));
        assert!(!result.contains('$'));
        // Patch wrapped the existing key and overwrote the other cell.
        assert!(result.contains("MT(KC_Q, LGui, hrm)"));
        assert!(result.contains("MO(2)              KC_LCTL") || result.contains("MO(2)"));
        // Combo and scene layer selectors became integers.
        assert!(result.contains("layer = 1"));
        assert!(result.contains("layer = 2"));
        // Profiles: identical one skipped, new one added.
        assert_eq!(
            result
                .matches("[behavior.morse.profiles.autoshift]")
                .count(),
            1
        );
        assert!(result.contains("[behavior.morse.profiles.hrm]"));
        assert!(notes.iter().any(|n| n.contains("layer $symbols -> slot 2")));
        assert!(notes.iter().any(|n| n.contains("already present, skipped")));
        // The merged config still parses and analyzes cleanly.
        let parsed = model::parse(&result).unwrap();
        assert_eq!(parsed.layers.len(), 3);
        let report = crate::usage::analyze(&parsed);
        assert!(
            report.warnings.is_empty(),
            "warnings: {:?}",
            report.warnings
        );
    }

    #[test]
    fn patch_key_requires_bare_keycode() {
        let preset = r#"[preset]
name = "bad"

[[patch]]
layer = 0

[[patch.key]]
at = [1, 2]
action = "MT($key, LGui, hrm)"
"#;
        let err = apply_preset(preset, TARGET).unwrap_err().to_string();
        assert!(err.contains("not a plain keycode"), "{err}");
    }

    #[test]
    fn patch_rejects_holes_and_out_of_range() {
        let hole = r#"[preset]
name = "bad"

[[patch]]
layer = 1

[[patch.key]]
at = [1, 0]
action = "KC_A"
"#;
        let err = apply_preset(hole, TARGET).unwrap_err().to_string();
        assert!(err.contains("physical hole"), "{err}");

        let out_of_range = r#"[preset]
name = "bad"

[[patch]]
layer = 0

[[patch.key]]
at = [0, 9]
action = "KC_A"
"#;
        let err = apply_preset(out_of_range, TARGET).unwrap_err().to_string();
        assert!(err.contains("beyond the grid"), "{err}");
    }

    #[test]
    fn unknown_reference_and_clashes_fail() {
        let unknown = r#"[preset]
name = "bad"

[[layer]]
id = "x"
keys = "MO($nope)"
"#;
        let err = apply_preset(unknown, TARGET).unwrap_err().to_string();
        assert!(err.contains("unknown preset reference $nope"), "{err}");

        let clash = r#"[preset]
name = "bad"

[[layer]]
id = "base"
keys = "KC_A"
"#;
        let err = apply_preset(clash, TARGET).unwrap_err().to_string();
        assert!(err.contains("already has a layer"), "{err}");

        let morse_clash = r#"[preset]
name = "bad"

[[morse]]
name = "Existing"
tap = "KC_A"
"#;
        let err = apply_preset(morse_clash, TARGET).unwrap_err().to_string();
        assert!(err.contains("already has a morse"), "{err}");
    }

    #[test]
    fn conflicting_profile_fails() {
        let preset = r#"[preset]
name = "bad"

[behavior.morse.profiles.autoshift]
hold_timeout_ms = 150
"#;
        let err = apply_preset(preset, TARGET).unwrap_err().to_string();
        assert!(
            err.contains("already exists with different settings"),
            "{err}"
        );
    }

    #[test]
    fn geometry_mismatch_fails() {
        let preset = r#"[preset]
name = "bad"
geometry = [6, 14]
"#;
        let err = apply_preset(preset, TARGET).unwrap_err().to_string();
        assert!(err.contains("6x14"), "{err}");
    }

    #[test]
    fn unsupported_sections_fail() {
        let preset = r#"[preset]
name = "bad"

[lighting.background]
enabled = false
"#;
        let err = apply_preset(preset, TARGET).unwrap_err().to_string();
        assert!(err.contains("[[lighting.scene]]"), "{err}");
    }

    #[test]
    fn shipped_presets_are_structurally_valid() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/presets");
        let mut seen = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            describe(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            seen += 1;
        }
        assert!(seen >= 4, "expected shipped presets in presets/");
    }

    /// A plain QWERTY-ish board of the given row count: every cell is a bare
    /// keycode, so `$key` patches apply anywhere.
    fn synthetic_target(rows: usize) -> String {
        let row = vec!["KC_Q"; 14].join("  ");
        let grid: Vec<String> = (0..rows).map(|_| row.clone()).collect();
        format!(
            "default_layer = 0\n\n[[layer]]\nid = \"base\"\nname = \"Base\"\nkeys = \"\"\"\n{}\n\"\"\"\n\n[lighting]\nbrightness = 255\n",
            grid.join("\n")
        )
    }

    #[test]
    fn scene_cells_need_a_lighting_section() {
        let preset = r##"[preset]
name = "scenes"

[[lighting.scene]]
layer = 0
key = [0, 0]
color = "#102030"
effect = "solid"
"##;
        let row = vec!["KC_Q"; 14].join("  ");
        let no_lighting = format!(
            "default_layer = 0\n\n[[layer]]\nid = \"base\"\nkeys = \"\"\"\n{}\n\"\"\"\n",
            vec![row.clone(); 6].join("\n")
        );
        let err = apply_preset(preset, &no_lighting).unwrap_err().to_string();
        assert!(err.contains("no [lighting] section"), "{err}");
        apply_preset(preset, &synthetic_target(6)).unwrap();
    }

    const PER_BOARD_PRESET: &str = r#"[preset]
name = "per-board"
boards = ["glove80", "go60"]

[[patch]]
layer = "default"

[[patch.key]]
at = { glove80 = "LH-C5R4", go60 = "LH-C5R3" }
action = "MT($key, LGui, hrm)"

[[patch.key]]
at = "RH-T2"
action = "KC_MUTE"
"#;

    #[test]
    fn per_board_addresses_resolve_for_each_board() {
        // Glove80: LH-C5R4 = [3, 1]; RH-T2 = [1, 7].
        let (merged, notes) = apply_preset(PER_BOARD_PRESET, &synthetic_target(6)).unwrap();
        assert!(notes.iter().any(|n| n.contains("target board: glove80")));
        assert!(notes.iter().any(|n| n.contains("[3,1]")), "{notes:?}");
        assert!(notes.iter().any(|n| n.contains("[1,7]")), "{notes:?}");
        assert!(merged.contains("MT(KC_Q, LGui, hrm)"));

        // Go60: LH-C5R3 = [2, 1].
        let (_, notes) = apply_preset(PER_BOARD_PRESET, &synthetic_target(5)).unwrap();
        assert!(notes.iter().any(|n| n.contains("target board: go60")));
        assert!(notes.iter().any(|n| n.contains("[2,1]")), "{notes:?}");
    }

    #[test]
    fn addresses_need_a_recognized_board() {
        // TARGET is a 2x3 grid — not a MoErgo shape.
        let preset = r#"[preset]
name = "addr"

[[patch]]
layer = 0

[[patch.key]]
at = "LH-C1R1"
action = "KC_A"
"#;
        let err = apply_preset(preset, TARGET).unwrap_err().to_string();
        assert!(err.contains("not a recognized MoErgo board"), "{err}");
    }

    #[test]
    fn boards_restriction_is_enforced() {
        let preset = r#"[preset]
name = "g80only"
boards = ["glove80"]
"#;
        let err = apply_preset(preset, &synthetic_target(5))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("supports glove80") && err.contains("go60"),
            "{err}"
        );
        apply_preset(preset, &synthetic_target(6)).unwrap();
    }

    #[test]
    fn per_board_layer_grids_pick_the_target_board() {
        let preset = r#"[preset]
name = "grids"
boards = ["glove80", "go60"]

[[layer]]
id = "extra"
name = "Extra"
keys.glove80 = """
KC_A  KC_B
"""
keys.go60 = """
KC_C  KC_D
"""
"#;
        let (merged, _) = apply_preset(preset, &synthetic_target(6)).unwrap();
        assert!(merged.contains("KC_A") && !merged.contains("KC_C"));
        let (merged, _) = apply_preset(preset, &synthetic_target(5)).unwrap();
        assert!(merged.contains("KC_C") && !merged.contains("KC_A"));
    }

    #[test]
    fn shipped_presets_apply_to_their_boards() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/presets");
        let read = |name: &str| std::fs::read_to_string(format!("{dir}/{name}.toml")).unwrap();
        let cases: [(&str, &[usize]); 4] = [
            ("hrm-bilateral", &[6, 5]),
            ("symbols-layer", &[6, 5]),
            ("magic-glove80", &[6]),
            ("magic-go60", &[5]),
        ];
        for (name, rows_list) in cases {
            let text = read(name);
            for &rows in rows_list {
                let target = synthetic_target(rows);
                let (merged, _) = apply_preset(&text, &target)
                    .unwrap_or_else(|e| panic!("{name} on {rows}-row target: {e}"));
                let parsed = model::parse(&merged).unwrap();
                let report = crate::usage::analyze(&parsed);
                assert!(
                    report.warnings.is_empty(),
                    "{name} on {rows}-row target introduced warnings: {:?}",
                    report.warnings
                );
            }
        }
        // The board-specific presets refuse the other board.
        let err = apply_preset(&read("magic-glove80"), &synthetic_target(5)).unwrap_err();
        assert!(err.to_string().contains("supports glove80"));
        let err = apply_preset(&read("magic-go60"), &synthetic_target(6)).unwrap_err();
        assert!(err.to_string().contains("supports go60"));
    }
}
