//! MoErgo physical section addresses. `LH`/`RH` name the half; finger
//! columns count from the thumb side outward (`C1`..`C6`) and rows from the
//! top down, so `RH-C1R3` is the right half's inner finger column, third
//! row. Thumb keys are `T1` onward (the Glove80 numbers its upper fan
//! first). The formula is shared by both boards — only the row count
//! differs, which is why the same physical key can have different row
//! numbers on the Glove80 (six rows) and the Go60 (five).

use anyhow::{Result, anyhow, bail};

use crate::model;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Board {
    Glove80,
    Go60,
}

impl Board {
    pub fn name(self) -> &'static str {
        match self {
            Board::Glove80 => "glove80",
            Board::Go60 => "go60",
        }
    }

    pub fn rows(self) -> usize {
        match self {
            Board::Glove80 => 6,
            Board::Go60 => 5,
        }
    }

    pub fn from_name(name: &str) -> Option<Board> {
        match name {
            "glove80" => Some(Board::Glove80),
            "go60" => Some(Board::Go60),
            _ => None,
        }
    }

    /// Identify the target board from its default layer's grid shape.
    pub fn detect(config: &model::Config) -> Option<Board> {
        let default = config.layers.get(config.default_layer)?;
        let rows = default.cells.iter().map(|c| c.row + 1).max()?;
        let cols = default.cells.iter().map(|c| c.col + 1).max()?;
        match (rows, cols) {
            (6, 14) => Some(Board::Glove80),
            (5, 14) => Some(Board::Go60),
            _ => None,
        }
    }
}

/// Resolve a section address to matrix `(row, col)` on the given board.
pub fn resolve(address: &str, board: Board) -> Result<(usize, usize)> {
    let err = || anyhow!("bad physical address \"{address}\" (LH-C4R4, RH-T2, …)");
    let (side, rest) = address.split_once('-').ok_or_else(err)?;
    let thumb_col = match side {
        "LH" => 6,
        "RH" => 7,
        _ => bail!("{} — the half must be LH or RH", err()),
    };

    let (row, col) = if let Some(thumb) = rest.strip_prefix('T') {
        let index: usize = thumb.parse().map_err(|_| err())?;
        if index == 0 || index > board.rows() {
            bail!(
                "{} — {} has thumb keys T1..T{}",
                err(),
                board.name(),
                board.rows()
            );
        }
        (index - 1, thumb_col)
    } else {
        let body = rest.strip_prefix('C').ok_or_else(err)?;
        let (column, row) = body.split_once('R').ok_or_else(err)?;
        let column: usize = column.parse().map_err(|_| err())?;
        let row: usize = row.parse().map_err(|_| err())?;
        if column == 0 || column > 6 {
            bail!("{} — finger columns are C1..C6", err());
        }
        if row == 0 || row > board.rows() {
            bail!(
                "{} — {} has rows R1..R{}",
                err(),
                board.name(),
                board.rows()
            );
        }
        let col = if side == "LH" { 6 - column } else { 7 + column };
        (row - 1, col)
    };
    Ok((row, col))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_fingers_and_thumbs_on_both_boards() {
        // Glove80 home row: A sits at [3, 1] = LH-C5R4.
        assert_eq!(resolve("LH-C5R4", Board::Glove80).unwrap(), (3, 1));
        assert_eq!(resolve("LH-C1R4", Board::Glove80).unwrap(), (3, 5));
        assert_eq!(resolve("RH-C1R3", Board::Glove80).unwrap(), (2, 8));
        assert_eq!(resolve("RH-C5R4", Board::Glove80).unwrap(), (3, 12));
        assert_eq!(resolve("LH-T3", Board::Glove80).unwrap(), (2, 6));
        assert_eq!(resolve("RH-T1", Board::Go60).unwrap(), (0, 7));
        // The Go60 has no F-row, so its home row is R3.
        assert_eq!(resolve("LH-C5R3", Board::Go60).unwrap(), (2, 1));
    }

    #[test]
    fn rejects_out_of_range_addresses() {
        assert!(resolve("LH-C5R6", Board::Go60).is_err());
        assert!(resolve("LH-C7R1", Board::Glove80).is_err());
        assert!(resolve("LH-T7", Board::Glove80).is_err());
        assert!(resolve("XH-C1R1", Board::Glove80).is_err());
        assert!(resolve("LH-C0R1", Board::Glove80).is_err());
        assert!(resolve("nonsense", Board::Glove80).is_err());
    }

    #[test]
    fn round_trips_the_formula() {
        // Every matrix cell formats to an address that resolves back.
        for board in [Board::Glove80, Board::Go60] {
            for row in 0..board.rows() {
                for col in 0..14 {
                    let address = if col == 6 {
                        format!("LH-T{}", row + 1)
                    } else if col == 7 {
                        format!("RH-T{}", row + 1)
                    } else if col < 6 {
                        format!("LH-C{}R{}", 6 - col, row + 1)
                    } else {
                        format!("RH-C{}R{}", col - 7, row + 1)
                    };
                    assert_eq!(resolve(&address, board).unwrap(), (row, col), "{address}");
                }
            }
        }
    }
}
