//! Commit object encoding.

use crate::odb::object::ObjectId;
use crate::util::time;
use crate::{FodeltaError, Result};

pub const COMMIT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    pub name: String,
    pub email: String,
    pub timestamp: i64,
    pub offset_minutes: i32,
}

impl Author {
    pub fn new(
        name: impl Into<String>,
        email: impl Into<String>,
        timestamp: i64,
        offset_minutes: i32,
    ) -> Result<Self> {
        let name = name.into();
        let email = email.into();
        if name.trim().is_empty() {
            return Err(invalid_author("author name is empty"));
        }
        if email.trim().is_empty() || email.contains(char::is_whitespace) {
            return Err(invalid_author(
                "author email is empty or contains whitespace",
            ));
        }
        if name.contains(['\n', '\r', '<', '>']) || email.contains(['\n', '\r', '<', '>']) {
            return Err(invalid_author(
                "author name or email contains an unsupported character",
            ));
        }
        Ok(Self {
            name,
            email,
            timestamp,
            offset_minutes,
        })
    }

    pub fn now(name: impl Into<String>, email: impl Into<String>) -> Result<Self> {
        let (timestamp, offset_minutes) = time::now();
        Self::new(name, email, timestamp, offset_minutes)
    }

    fn encode_line(&self) -> String {
        format!(
            "author {} <{}> {} {}",
            self.name,
            self.email,
            self.timestamp,
            time::format_offset(self.offset_minutes)
        )
    }

    fn parse(line: &str) -> Result<Self> {
        let rest = line
            .strip_prefix("author ")
            .ok_or_else(|| invalid_commit("author line is malformed"))?;
        let open = rest
            .rfind('<')
            .ok_or_else(|| invalid_commit("author line is missing '<'"))?;
        let close = rest
            .rfind('>')
            .ok_or_else(|| invalid_commit("author line is missing '>'"))?;
        if close < open {
            return Err(invalid_commit("author line has mismatched brackets"));
        }
        let name = rest[..open].trim_end();
        let email = &rest[open + 1..close];
        let tail = rest[close + 1..].trim();
        let mut parts = tail.split_whitespace();
        let timestamp_text = parts
            .next()
            .ok_or_else(|| invalid_commit("author line is missing a timestamp"))?;
        let offset_text = parts
            .next()
            .ok_or_else(|| invalid_commit("author line is missing an offset"))?;
        if parts.next().is_some() {
            return Err(invalid_commit("author line has trailing fields"));
        }
        let timestamp: i64 = timestamp_text
            .parse()
            .map_err(|_| invalid_commit("author timestamp is not a number"))?;
        let offset_minutes = time::parse_offset(offset_text)
            .ok_or_else(|| invalid_commit("author offset is malformed"))?;
        Self::new(name, email, timestamp, offset_minutes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub tree: ObjectId,
    pub parent: Option<ObjectId>,
    pub author: Author,
    pub message: String,
}

impl Commit {
    pub fn new(tree: ObjectId, parent: Option<ObjectId>, author: Author, message: String) -> Self {
        Self {
            tree,
            parent,
            author,
            message,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut out = String::new();
        out.push_str("tree ");
        out.push_str(&self.tree.to_hex());
        out.push('\n');
        if let Some(parent) = self.parent {
            out.push_str("parent ");
            out.push_str(&parent.to_hex());
            out.push('\n');
        }
        out.push_str(&self.author.encode_line());
        out.push('\n');
        out.push_str("fodelta ");
        out.push_str(&COMMIT_VERSION.to_string());
        out.push('\n');
        out.push('\n');
        out.push_str(&self.message);
        Ok(out.into_bytes())
    }

    pub fn decode(payload: &[u8]) -> Result<Self> {
        let text = std::str::from_utf8(payload)
            .map_err(|_| invalid_commit("commit payload is not valid UTF-8"))?;
        let split = text
            .find("\n\n")
            .ok_or_else(|| invalid_commit("commit is missing the header/message separator"))?;
        let headers = &text[..split];
        let message = &text[split + 2..];

        let mut tree = None;
        let mut parent = None;
        let mut author = None;
        let mut version = None;

        for line in headers.lines() {
            let (key, value) = line
                .split_once(' ')
                .ok_or_else(|| invalid_commit(format!("malformed commit header: {line}")))?;
            match key {
                "tree" => {
                    if tree.replace(ObjectId::from_hex(value)?).is_some() {
                        return Err(invalid_commit("duplicate tree header"));
                    }
                }
                "parent" => {
                    if parent.replace(ObjectId::from_hex(value)?).is_some() {
                        return Err(invalid_commit("commit has more than one parent"));
                    }
                }
                "author" => {
                    if author.replace(Author::parse(line)?).is_some() {
                        return Err(invalid_commit("duplicate author header"));
                    }
                }
                "fodelta" => {
                    let parsed: u32 = value
                        .parse()
                        .map_err(|_| invalid_commit("commit version is not a number"))?;
                    if version.replace(parsed).is_some() {
                        return Err(invalid_commit("duplicate fodelta header"));
                    }
                }
                other => {
                    return Err(invalid_commit(format!("unknown commit header '{other}'")));
                }
            }
        }

        let tree = tree.ok_or_else(|| invalid_commit("commit is missing a tree"))?;
        let author = author.ok_or_else(|| invalid_commit("commit is missing an author"))?;
        let version = version.ok_or_else(|| invalid_commit("commit is missing a version"))?;
        if version != COMMIT_VERSION {
            return Err(invalid_commit(format!(
                "unsupported commit version {version} (expected {COMMIT_VERSION})"
            )));
        }
        Ok(Self {
            tree,
            parent,
            author,
            message: message.to_string(),
        })
    }
}

fn invalid_commit(reason: impl Into<String>) -> FodeltaError {
    FodeltaError::InvalidObject {
        reason: reason.into(),
    }
}

fn invalid_author(reason: impl Into<String>) -> FodeltaError {
    FodeltaError::InvalidObject {
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oid(byte: u8) -> ObjectId {
        ObjectId::from_bytes([byte; 32])
    }

    fn author() -> Author {
        Author::new("Ada Lovelace", "ada@example.com", 1_780_000_000, 480).unwrap()
    }

    #[test]
    fn roundtrip_root_commit() {
        let commit = Commit::new(oid(1), None, author(), "initial\n".to_string());
        let payload = commit.encode().unwrap();
        assert_eq!(Commit::decode(&payload).unwrap(), commit);
        assert!(
            String::from_utf8(payload)
                .unwrap()
                .contains("author Ada Lovelace <ada@example.com> 1780000000 +0800")
        );
    }

    #[test]
    fn roundtrip_child_commit_with_body() {
        let commit = Commit::new(
            oid(2),
            Some(oid(1)),
            author(),
            "subject\n\nbody line\n".to_string(),
        );
        let payload = commit.encode().unwrap();
        let decoded = Commit::decode(&payload).unwrap();
        assert_eq!(decoded, commit);
        assert_eq!(decoded.parent, Some(oid(1)));
        assert!(decoded.message.contains("body line"));
    }

    #[test]
    fn rejects_bad_versions_and_headers() {
        let text = b"tree 1111111111111111111111111111111111111111111111111111111111111111\nauthor A <a@b> 1 +0000\nfodelta 2\n\nx";
        assert!(Commit::decode(text).is_err());
        let text = b"tree 1111111111111111111111111111111111111111111111111111111111111111\nauthor A <a@b> 1 +0000\nfodelta 1\n";
        assert!(Commit::decode(text).is_err());
        let text = b"tree 1111111111111111111111111111111111111111111111111111111111111111\nparent 2222222222222222222222222222222222222222222222222222222222222222\nparent 3333333333333333333333333333333333333333333333333333333333333333\nauthor A <a@b> 1 +0000\nfodelta 1\n\nx";
        assert!(Commit::decode(text).is_err());
    }

    #[test]
    fn author_rejects_newlines() {
        assert!(Author::new("A\nB", "a@b", 0, 0).is_err());
        assert!(Author::new("A", "a b@c", 0, 0).is_err());
    }
}
