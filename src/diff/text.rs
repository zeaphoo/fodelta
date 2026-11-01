//! Line based diffing and unified rendering.

use similar::{Algorithm, DiffOp, capture_diff_slices, capture_diff_slices_by_key};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffAlgorithm {
    Myers,
    Patience,
    Histogram,
}

impl DiffAlgorithm {
    pub fn from_config(value: &str) -> Self {
        match value {
            "patience" => Self::Patience,
            "histogram" => Self::Histogram,
            _ => Self::Myers,
        }
    }

    fn similar(self) -> Algorithm {
        match self {
            Self::Myers => Algorithm::Myers,
            Self::Patience => Algorithm::Patience,
            Self::Histogram => Algorithm::Histogram,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WhitespaceOptions {
    pub ignore_all: bool,
    pub ignore_change: bool,
    pub ignore_eol: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LineStats {
    pub added: usize,
    pub removed: usize,
}

#[derive(Debug, Clone, Copy)]
enum Tag {
    Equal,
    Delete(usize),
    Insert(usize),
}

pub fn is_binary(bytes: &[u8]) -> bool {
    if bytes.iter().take(8000).any(|byte| *byte == 0) {
        return true;
    }
    std::str::from_utf8(bytes).is_err()
}

/// Render a unified diff with `context` lines around each hunk.
pub fn render_unified(
    old_label: &str,
    new_label: &str,
    old: &[u8],
    new: &[u8],
    context: usize,
    algorithm: DiffAlgorithm,
    whitespace: WhitespaceOptions,
) -> (String, LineStats) {
    let old_text = String::from_utf8_lossy(old);
    let new_text = String::from_utf8_lossy(new);
    let old_lines = split_lines(&old_text);
    let new_lines = split_lines(&new_text);
    let ops = if whitespace == WhitespaceOptions::default() {
        capture_diff_slices(algorithm.similar(), &old_lines, &new_lines)
    } else {
        capture_diff_slices_by_key(algorithm.similar(), &old_lines, &new_lines, |line| {
            normalize_line(line, whitespace)
        })
    };
    let (tags, stats) = tags_from_ops(&ops);

    let mut out = String::new();
    out.push_str("--- ");
    out.push_str(old_label);
    out.push('\n');
    out.push_str("+++ ");
    out.push_str(new_label);
    out.push('\n');

    let change_indices: Vec<usize> = tags
        .iter()
        .enumerate()
        .filter(|(_, tag)| !matches!(tag, Tag::Equal))
        .map(|(index, _)| index)
        .collect();
    if change_indices.is_empty() {
        return (out, stats);
    }

    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut start = change_indices[0].saturating_sub(context);
    let mut end = (change_indices[0] + context + 1).min(tags.len());
    for index in change_indices.into_iter().skip(1) {
        let candidate_start = index.saturating_sub(context);
        let candidate_end = (index + context + 1).min(tags.len());
        if candidate_start <= end {
            end = end.max(candidate_end);
        } else {
            ranges.push((start, end));
            start = candidate_start;
            end = candidate_end;
        }
    }
    ranges.push((start, end));

    let (old_positions, new_positions) = positions(&tags);
    for (range_start, range_end) in ranges {
        let old_count = old_positions[range_end] - old_positions[range_start];
        let new_count = new_positions[range_end] - new_positions[range_start];
        let old_start = if old_count == 0 {
            old_positions[range_start]
        } else {
            old_positions[range_start] + 1
        };
        let new_start = if new_count == 0 {
            new_positions[range_start]
        } else {
            new_positions[range_start] + 1
        };
        out.push_str(&format!(
            "@@ -{} +{} @@\n",
            range(old_start, old_count),
            range(new_start, new_count)
        ));
        let mut old_cursor = old_positions[range_start];
        for tag in &tags[range_start..range_end] {
            match *tag {
                Tag::Equal => {
                    push_line(&mut out, ' ', old_lines[old_cursor]);
                    old_cursor += 1;
                }
                Tag::Delete(index) => {
                    push_line(&mut out, '-', old_lines[index]);
                    old_cursor += 1;
                }
                Tag::Insert(index) => push_line(&mut out, '+', new_lines[index]),
            }
        }
    }
    (out, stats)
}

fn tags_from_ops(ops: &[DiffOp]) -> (Vec<Tag>, LineStats) {
    let mut tags = Vec::new();
    let mut stats = LineStats::default();
    for op in ops {
        match *op {
            DiffOp::Equal { len, .. } => tags.extend(std::iter::repeat_n(Tag::Equal, len)),
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                for index in old_index..old_index + old_len {
                    tags.push(Tag::Delete(index));
                }
                stats.removed += old_len;
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                for index in new_index..new_index + new_len {
                    tags.push(Tag::Insert(index));
                }
                stats.added += new_len;
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                for index in old_index..old_index + old_len {
                    tags.push(Tag::Delete(index));
                }
                for index in new_index..new_index + new_len {
                    tags.push(Tag::Insert(index));
                }
                stats.removed += old_len;
                stats.added += new_len;
            }
        }
    }
    (tags, stats)
}

fn positions(tags: &[Tag]) -> (Vec<usize>, Vec<usize>) {
    let mut old_positions = Vec::with_capacity(tags.len() + 1);
    let mut new_positions = Vec::with_capacity(tags.len() + 1);
    let mut old = 0;
    let mut new = 0;
    for tag in tags {
        old_positions.push(old);
        new_positions.push(new);
        match tag {
            Tag::Equal => {
                old += 1;
                new += 1;
            }
            Tag::Delete(_) => old += 1,
            Tag::Insert(_) => new += 1,
        }
    }
    old_positions.push(old);
    new_positions.push(new);
    (old_positions, new_positions)
}

fn push_line(out: &mut String, prefix: char, line: &str) {
    out.push(prefix);
    out.push_str(line);
    if !line.ends_with('\n') {
        out.push('\n');
        out.push_str("\\ No newline at end of file\n");
    }
}

fn range(start: usize, count: usize) -> String {
    if count == 1 {
        start.to_string()
    } else {
        format!("{start},{count}")
    }
}

fn normalize_line(line: &str, whitespace: WhitespaceOptions) -> String {
    if whitespace.ignore_all {
        line.chars()
            .filter(|character| !character.is_whitespace())
            .collect()
    } else if whitespace.ignore_change {
        line.split_whitespace().collect::<Vec<_>>().join(" ")
    } else if whitespace.ignore_eol {
        line.trim_end().to_string()
    } else {
        line.to_string()
    }
}

/// Split on `\n` while preserving the terminator, including CRLF.
fn split_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            lines.push(&text[start..=index]);
            start = index + 1;
        }
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_simple_hunk() {
        let (diff, stats) = render_unified(
            "a/file.txt",
            "b/file.txt",
            b"one\ntwo\nthree\n",
            b"one\nTWO\nthree\n",
            3,
            DiffAlgorithm::Myers,
            WhitespaceOptions::default(),
        );
        assert!(diff.contains("--- a/file.txt"));
        assert!(diff.contains("+++ b/file.txt"));
        assert!(diff.contains("-two"));
        assert!(diff.contains("+TWO"));
        assert_eq!(stats.added, 1);
        assert_eq!(stats.removed, 1);
    }

    #[test]
    fn context_of_zero_omits_unchanged_lines() {
        let (diff, _) = render_unified(
            "a",
            "b",
            b"one\ntwo\n",
            b"one\nTWO\n",
            0,
            DiffAlgorithm::Myers,
            WhitespaceOptions::default(),
        );
        assert!(!diff.contains(" one"));
        assert!(diff.contains("-two"));
    }

    #[test]
    fn preserves_crlf_in_unchanged_lines() {
        let (diff, _) = render_unified(
            "a",
            "b",
            b"one\r\ntwo\r\n",
            b"one\r\nTWO\r\n",
            3,
            DiffAlgorithm::Myers,
            WhitespaceOptions::default(),
        );
        assert!(diff.contains(" one\r\n"), "{diff:?}");
    }

    #[test]
    fn detects_binary_and_invalid_utf8() {
        assert!(is_binary(b"a\0b"));
        assert!(is_binary(&[0xff, 0xfe]));
        assert!(!is_binary(b"hello\n"));
    }
}
