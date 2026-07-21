use std::collections::{HashMap, HashSet};

pub const ATTENTION_LEDS: [u8; 3] = [34, 28, 22];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Source {
    Codex,
    Claude,
    Overflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Approval,
    Input,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attention {
    pub id: String,
    pub source: Source,
    pub kind: Kind,
    sequence: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Signal {
    pub led: u8,
    pub color: &'static str,
    pub effect: &'static str,
    pub period_ms: u16,
    pub duty_percent: Option<u8>,
}

#[derive(Debug, Default)]
pub struct AttentionState {
    entries: HashMap<String, Attention>,
    next_sequence: u64,
}

impl AttentionState {
    pub fn set(&mut self, id: impl Into<String>, source: Source, kind: Kind) {
        let id = id.into();
        if let Some(existing) = self.entries.get_mut(&id) {
            existing.source = source;
            existing.kind = kind;
            return;
        }

        let sequence = self.next_sequence;
        self.next_sequence += 1;
        self.entries.insert(
            id.clone(),
            Attention {
                id,
                source,
                kind,
                sequence,
            },
        );
    }

    pub fn remove(&mut self, id: &str) {
        self.entries.remove(id);
    }

    pub fn replace_codex(&mut self, pending: impl IntoIterator<Item = (String, Kind)>) {
        let pending: Vec<_> = pending.into_iter().collect();
        let desired: HashSet<_> = pending.iter().map(|(id, _)| id.as_str()).collect();
        self.entries
            .retain(|id, entry| entry.source != Source::Codex || desired.contains(id.as_str()));
        for (id, kind) in pending {
            self.set(id, Source::Codex, kind);
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn signals(&self) -> [Option<Signal>; 3] {
        let mut pending: Vec<_> = self.entries.values().collect();
        pending.sort_by_key(|entry| {
            let priority = match entry.kind {
                Kind::Approval => 0,
                Kind::Input => 1,
            };
            (priority, entry.sequence)
        });

        let mut signals = [None, None, None];
        let visible_count = pending.len().min(ATTENTION_LEDS.len());
        for index in 0..visible_count {
            signals[index] = Some(signal_for(
                ATTENTION_LEDS[index],
                pending[index].source,
                pending[index].kind,
            ));
        }

        if pending.len() > ATTENTION_LEDS.len() {
            signals[2] = Some(signal_for(
                ATTENTION_LEDS[2],
                Source::Overflow,
                Kind::Approval,
            ));
        }
        signals
    }
}

fn signal_for(led: u8, source: Source, kind: Kind) -> Signal {
    let color = match source {
        Source::Codex => "00aaff",
        Source::Claude => "ff8800",
        Source::Overflow => "ff00ff",
    };
    match kind {
        Kind::Approval => Signal {
            led,
            color,
            effect: "blink",
            period_ms: 600,
            duty_percent: Some(40),
        },
        Kind::Input => Signal {
            led,
            color,
            effect: "breathe",
            period_ms: 1800,
            duty_percent: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approvals_sort_before_input_then_by_arrival() {
        let mut state = AttentionState::default();
        state.set("claude:one", Source::Claude, Kind::Input);
        state.set("codex:one", Source::Codex, Kind::Approval);

        let signals = state.signals();
        assert_eq!(signals[0].unwrap().led, 34);
        assert_eq!(signals[0].unwrap().color, "00aaff");
        assert_eq!(signals[0].unwrap().effect, "blink");
        assert_eq!(signals[1].unwrap().color, "ff8800");
        assert_eq!(signals[1].unwrap().effect, "breathe");
    }

    #[test]
    fn overflow_uses_the_third_led() {
        let mut state = AttentionState::default();
        for index in 0..4 {
            state.set(format!("codex:{index}"), Source::Codex, Kind::Input);
        }

        let signals = state.signals();
        assert_eq!(signals[2].unwrap().led, 22);
        assert_eq!(signals[2].unwrap().color, "ff00ff");
        assert_eq!(signals[2].unwrap().effect, "blink");
    }

    #[test]
    fn codex_snapshot_preserves_claude_and_removes_stale_codex() {
        let mut state = AttentionState::default();
        state.set("codex:old", Source::Codex, Kind::Input);
        state.set("claude:still", Source::Claude, Kind::Input);

        state.replace_codex([("codex:new".to_owned(), Kind::Approval)]);

        assert_eq!(state.len(), 2);
        assert!(!state.entries.contains_key("codex:old"));
        assert!(state.entries.contains_key("codex:new"));
        assert!(state.entries.contains_key("claude:still"));
    }
}
