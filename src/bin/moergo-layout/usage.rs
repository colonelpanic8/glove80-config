//! Behavior usage report: where every layer, morse, macro, fork, and morse
//! profile is referenced, which are orphaned, and which layers are
//! unreachable from the default layer.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::model::{Config, Token};

pub struct Report {
    pub text: String,
    pub warnings: Vec<String>,
}

struct Site {
    /// Layer index this site is usable from; `None` means any layer.
    from_layer: Option<usize>,
    location: String,
    token: Token,
}

fn sites(config: &Config) -> Vec<Site> {
    let mut sites = Vec::new();
    for layer in &config.layers {
        for cell in &layer.cells {
            sites.push(Site {
                from_layer: Some(layer.index),
                location: format!("L{} r{}c{}", layer.index, cell.row, cell.col),
                token: cell.token.clone(),
            });
        }
        for bind in &layer.binds {
            sites.push(Site {
                from_layer: Some(layer.index),
                location: format!("L{} bind", layer.index),
                token: bind.clone(),
            });
        }
    }
    for combo in &config.combos {
        for token in combo.keys.iter().chain(&combo.output) {
            sites.push(Site {
                from_layer: combo.layer,
                location: format!("combo \"{}\"", combo.name),
                token: token.clone(),
            });
        }
    }
    for fork in &config.forks {
        for token in fork.trigger.iter().chain(&fork.output) {
            sites.push(Site {
                from_layer: None,
                location: format!("fork \"{}\"", fork.name),
                token: token.clone(),
            });
        }
    }
    sites
}

pub fn analyze(config: &Config) -> Report {
    let mut out = String::new();
    let mut warnings = Vec::new();
    let sites = sites(config);
    let defined: BTreeSet<usize> = config.layers.iter().map(|l| l.index).collect();

    // Expand morse references: a TD(n) site also stands for morse n's actions.
    let mut effective: Vec<(Option<usize>, String, Token)> = Vec::new();
    for site in &sites {
        effective.push((site.from_layer, site.location.clone(), site.token.clone()));
        if let Some(index) = site.token.morse_ref()
            && let Some(morse) = config.morses.iter().find(|m| m.index == index)
        {
            for (field, action) in &morse.actions {
                effective.push((
                    site.from_layer,
                    format!("morse {index} {field} via {}", site.location),
                    action.clone(),
                ));
            }
        }
    }

    // Layer activators and the reachability graph.
    let mut activators: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    let mut edges: BTreeMap<Option<usize>, BTreeSet<usize>> = BTreeMap::new();
    for (from, location, token) in &effective {
        for (kind, target) in token.layer_refs() {
            activators
                .entry(target)
                .or_default()
                .push(format!("{kind} at {location}"));
            edges.entry(*from).or_default().insert(target);
        }
    }

    let mut reachable: BTreeSet<usize> = BTreeSet::new();
    let mut queue = VecDeque::from([config.default_layer]);
    // Layer-less sites (global combos, forks) can fire from any active layer.
    let global: Vec<usize> = edges
        .get(&None)
        .map(|s| s.iter().copied().collect())
        .unwrap_or_default();
    while let Some(layer) = queue.pop_front() {
        if !reachable.insert(layer) {
            continue;
        }
        for target in edges.get(&Some(layer)).into_iter().flatten() {
            queue.push_back(*target);
        }
        for target in &global {
            queue.push_back(*target);
        }
    }

    out.push_str(&format!(
        "Layers ({} defined, default {}):\n",
        config.layers.len(),
        config.default_layer
    ));
    for layer in &config.layers {
        let bound = layer.cells.iter().filter(|c| c.token.is_bound()).count();
        let scenes = config.scene_cells.get(&layer.index).copied().unwrap_or(0);
        let mut notes = Vec::new();
        if layer.index == config.default_layer {
            notes.push("default".to_string());
        }
        if config.wake_layers.contains(&layer.index) {
            notes.push("wake".to_string());
        }
        if !reachable.contains(&layer.index) {
            notes.push("UNREACHABLE".to_string());
        }
        let notes = if notes.is_empty() {
            String::new()
        } else {
            format!("  [{}]", notes.join(", "))
        };
        out.push_str(&format!(
            "  {:>2} {:<18} {:>3} bound keys  {:>3} scene cells{}\n",
            layer.index, layer.name, bound, scenes, notes
        ));
        match activators.get(&layer.index) {
            Some(list) => {
                out.push_str(&format!("     activated by: {}\n", summarize(list, 6)));
            }
            None if layer.index != config.default_layer => {
                out.push_str("     activated by: nothing\n");
                warnings.push(format!(
                    "layer {} \"{}\" has no activator",
                    layer.index, layer.name
                ));
            }
            None => {}
        }
        if !reachable.contains(&layer.index) {
            warnings.push(format!(
                "layer {} \"{}\" is unreachable from default layer {}",
                layer.index, layer.name, config.default_layer
            ));
        }
    }

    for (target, list) in &activators {
        if !defined.contains(target) {
            warnings.push(format!(
                "layer {} is referenced ({}) but has no [[layer]] entry",
                target,
                summarize(list, 3)
            ));
        }
    }
    for layer in config.scene_cells.keys() {
        if !defined.contains(layer) {
            warnings.push(format!(
                "lighting scene targets layer {layer}, which has no [[layer]] entry"
            ));
        }
    }

    // Morse usage.
    if !config.morses.is_empty() || effective.iter().any(|(_, _, t)| t.morse_ref().is_some()) {
        out.push_str(&format!("\nMorse behaviors ({}):\n", config.morses.len()));
        let mut referenced: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        for site in &sites {
            if let Some(index) = site.token.morse_ref() {
                referenced
                    .entry(index)
                    .or_default()
                    .push(site.location.clone());
            }
        }
        for morse in &config.morses {
            let actions: Vec<String> = morse
                .actions
                .iter()
                .map(|(f, t)| format!("{f} {}", t.0))
                .collect();
            match referenced.get(&morse.index) {
                Some(list) => out.push_str(&format!(
                    "  TD({}) \"{}\" ({})  used {}x: {}\n",
                    morse.index,
                    morse.name,
                    actions.join(", "),
                    list.len(),
                    summarize(list, 4)
                )),
                None => {
                    out.push_str(&format!(
                        "  TD({}) \"{}\" ({})  UNUSED\n",
                        morse.index,
                        morse.name,
                        actions.join(", ")
                    ));
                    warnings.push(format!(
                        "morse {} \"{}\" is never referenced",
                        morse.index, morse.name
                    ));
                }
            }
        }
        for (index, list) in &referenced {
            if !config.morses.iter().any(|m| m.index == *index) {
                warnings.push(format!(
                    "TD({}) is referenced ({}) but morse {} is not defined",
                    index,
                    summarize(list, 3),
                    index
                ));
            }
        }
    }

    // Macro usage.
    if !config.macros.is_empty() || effective.iter().any(|(_, _, t)| t.macro_ref().is_some()) {
        out.push_str(&format!("\nMacros ({}):\n", config.macros.len()));
        let mut referenced: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        for (_, location, token) in &effective {
            if let Some(index) = token.macro_ref() {
                referenced.entry(index).or_default().push(location.clone());
            }
        }
        for mac in &config.macros {
            match referenced.get(&mac.index) {
                Some(list) => out.push_str(&format!(
                    "  MACRO({}) \"{}\" ({} ops)  used {}x: {}\n",
                    mac.index,
                    mac.name,
                    mac.operation_count,
                    list.len(),
                    summarize(list, 4)
                )),
                None => {
                    out.push_str(&format!(
                        "  MACRO({}) \"{}\" ({} ops)  UNUSED\n",
                        mac.index, mac.name, mac.operation_count
                    ));
                    warnings.push(format!(
                        "macro {} \"{}\" is never referenced",
                        mac.index, mac.name
                    ));
                }
            }
        }
        for (index, list) in &referenced {
            if !config.macros.iter().any(|m| m.index == *index) {
                warnings.push(format!(
                    "MACRO({}) is referenced ({}) but macro {} is not defined",
                    index,
                    summarize(list, 3),
                    index
                ));
            }
        }
    }

    // Morse profile usage.
    if !config.profiles.is_empty() {
        out.push_str(&format!("\nMorse profiles ({}):\n", config.profiles.len()));
        let mut referenced: BTreeMap<&str, usize> = BTreeMap::new();
        for (_, _, token) in &effective {
            if let Some(profile) = token.profile_ref() {
                *referenced.entry(profile).or_default() += 1;
            }
        }
        for profile in &config.profiles {
            match referenced.get(profile.as_str()) {
                Some(count) => {
                    out.push_str(&format!("  {profile}  used {count}x\n"));
                }
                None => {
                    out.push_str(&format!("  {profile}  UNUSED\n"));
                    warnings.push(format!("morse profile \"{profile}\" is never referenced"));
                }
            }
        }
        for (profile, _) in referenced {
            if !config.profiles.iter().any(|p| p == profile) {
                warnings.push(format!(
                    "morse profile \"{profile}\" is referenced but not defined \
                     under [behavior.morse.profiles]"
                ));
            }
        }
    }

    // Combos are triggered positionally; report layer placement.
    if !config.combos.is_empty() {
        out.push_str(&format!("\nCombos ({}):\n", config.combos.len()));
        let mut by_layer: BTreeMap<Option<usize>, usize> = BTreeMap::new();
        for combo in &config.combos {
            *by_layer.entry(combo.layer).or_default() += 1;
            if let Some(layer) = combo.layer {
                if !defined.contains(&layer) {
                    warnings.push(format!(
                        "combo \"{}\" targets layer {}, which has no [[layer]] entry",
                        combo.name, layer
                    ));
                } else if !reachable.contains(&layer) {
                    warnings.push(format!(
                        "combo \"{}\" lives on unreachable layer {}",
                        combo.name, layer
                    ));
                }
            }
        }
        for (layer, count) in by_layer {
            match layer {
                Some(layer) => out.push_str(&format!("  layer {layer}: {count}\n")),
                None => out.push_str(&format!("  all layers: {count}\n")),
            }
        }
    }

    // USER() hook usage.
    let mut users: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for (_, location, token) in &effective {
        if let Some(index) = token.user_ref() {
            users.entry(index).or_default().push(location.clone());
        }
    }
    if !users.is_empty() {
        out.push_str("\nUSER hooks:\n");
        for (index, list) in users {
            out.push_str(&format!(
                "  USER({index})  used {}x: {}\n",
                list.len(),
                summarize(&list, 4)
            ));
        }
    }

    Report {
        text: out,
        warnings,
    }
}

/// Occurrences of every base keycode identifier, including inside wrappers.
pub fn keycode_histogram(config: &Config) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for site in sites(config) {
        for ident in identifiers(&site.token.0) {
            if ident.starts_with("KC_") && ident != "KC_TRNS" && ident != "KC_NO" {
                *counts.entry(ident).or_default() += 1;
            }
        }
    }
    let mut sorted: Vec<(String, usize)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    sorted
}

fn identifiers(raw: &str) -> Vec<String> {
    let mut idents = Vec::new();
    let mut current = String::new();
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            current.push(c);
        } else if !current.is_empty() {
            idents.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        idents.push(current);
    }
    idents
}

fn summarize(sites: &[String], max: usize) -> String {
    let mut text = sites
        .iter()
        .take(max)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if sites.len() > max {
        text.push_str(&format!(", … {} more", sites.len() - max));
    }
    text
}

#[cfg(test)]
mod tests {
    use crate::model::parse;

    use super::*;

    const FIXTURE: &str = r#"
default_layer = 0

[[layer]]
name = "Base"
keys = """
KC_A MO(1) TD(0) MACRO(0)
"""

[[layer]]
name = "Lower"
keys = """
KC_B TO(0) MACRO(2) _______
"""

[[layer]]
name = "Island"
keys = """
KC_C -- -- --
"""

[[morse]]
name = "used"
tap = "KC_DEL"
hold = "MO(1)"

[[morse]]
name = "orphan"
tap = "KC_X"

[[macro]]
name = "used"
[[macro.operations]]
operation = "tap"
keycode = "KC_HOME"

[[macro]]
name = "orphan"
[[macro.operations]]
operation = "tap"
keycode = "KC_END"

[behavior.morse.profiles.unused_profile]
hold_timeout_ms = 200
"#;

    #[test]
    fn finds_orphans_and_unreachable_layers() {
        let config = parse(FIXTURE).unwrap();
        let report = analyze(&config);
        let has = |needle: &str| report.warnings.iter().any(|w| w.contains(needle));
        assert!(
            has("layer 2 \"Island\" has no activator"),
            "{:?}",
            report.warnings
        );
        assert!(
            has("layer 2 \"Island\" is unreachable"),
            "{:?}",
            report.warnings
        );
        assert!(
            has("morse 1 \"orphan\" is never referenced"),
            "{:?}",
            report.warnings
        );
        assert!(
            has("macro 1 \"orphan\" is never referenced"),
            "{:?}",
            report.warnings
        );
        assert!(has("MACRO(2) is referenced"), "{:?}", report.warnings);
        assert!(
            has("morse profile \"unused_profile\""),
            "{:?}",
            report.warnings
        );
        assert!(!has("morse 0"), "{:?}", report.warnings);
        assert!(!has("macro 0"), "{:?}", report.warnings);
    }

    #[test]
    fn morse_hold_counts_as_layer_activator() {
        let config = parse(FIXTURE).unwrap();
        let report = analyze(&config);
        assert!(
            !report.warnings.iter().any(|w| w.contains("layer 1")),
            "layer 1 is activated by MO(1) and morse 0 hold: {:?}",
            report.warnings
        );
    }

    #[test]
    fn histogram_counts_nested_keycodes() {
        let config = parse(
            r#"
[[layer]]
name = "Base"
keys = """
KC_A TH(KC_Q, LSFT(KC_Q), autoshift)
"""
"#,
        )
        .unwrap();
        let histogram = keycode_histogram(&config);
        assert!(histogram.contains(&("KC_Q".to_string(), 2)));
        assert!(histogram.contains(&("KC_A".to_string(), 1)));
    }
}
