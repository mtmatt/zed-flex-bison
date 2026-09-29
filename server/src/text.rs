//! Conversions between byte offsets and LSP (UTF-16) positions.

use lsp_types::{Position, Range};

pub type Span = std::ops::Range<usize>;

#[derive(Clone, Debug)]
pub struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(
            text.bytes()
                .enumerate()
                .filter(|&(_, b)| b == b'\n')
                .map(|(i, _)| i + 1),
        );
        Self { line_starts }
    }

    pub fn position(&self, text: &str, offset: usize) -> Position {
        let offset = offset.min(text.len());
        let line = self.line_starts.partition_point(|&s| s <= offset) - 1;
        let start = self.line_starts[line];
        let col: usize = text[start..offset].chars().map(char::len_utf16).sum();
        Position::new(line as u32, col as u32)
    }

    pub fn offset(&self, text: &str, pos: Position) -> usize {
        let line = pos.line as usize;
        let Some(&start) = self.line_starts.get(line) else {
            return text.len();
        };
        let end = self
            .line_starts
            .get(line + 1)
            .copied()
            .unwrap_or(text.len());
        let mut col = 0u32;
        for (i, c) in text[start..end].char_indices() {
            if col >= pos.character || c == '\n' {
                return start + i;
            }
            col += c.len_utf16() as u32;
        }
        end
    }

    pub fn range(&self, text: &str, span: &Span) -> Range {
        Range::new(
            self.position(text, span.start),
            self.position(text, span.end),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_ascii_and_utf16() {
        let text = "ab\nc😀d\n";
        let idx = LineIndex::new(text);
        assert_eq!(idx.position(text, 4), Position::new(1, 1));
        // 😀 is 4 bytes in UTF-8 and 2 code units in UTF-16.
        assert_eq!(idx.position(text, 8), Position::new(1, 3));
        assert_eq!(idx.offset(text, Position::new(1, 3)), 8);
        assert_eq!(idx.offset(text, Position::new(0, 99)), 2);
        assert_eq!(idx.offset(text, Position::new(9, 0)), text.len());
    }
}
