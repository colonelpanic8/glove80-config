use crate::attention::{Kind, Source};

#[derive(Debug)]
pub enum Event {
    Set {
        id: String,
        source: Source,
        kind: Kind,
    },
    Remove {
        id: String,
    },
    ReplaceCodex(Vec<(String, Kind)>),
}
