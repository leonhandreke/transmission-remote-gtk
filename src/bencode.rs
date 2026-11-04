//! Decoding .torrent files (bencode.c and trg-file-parser.c): their name and
//! the tree of their files, for the add dialog.

use std::collections::BTreeMap;

use crate::files::FileNode;

#[derive(Debug, Clone, PartialEq)]
pub enum Be {
    Int(i64),
    Str(Vec<u8>),
    List(Vec<Be>),
    Dict(BTreeMap<Vec<u8>, Be>),
}

impl Be {
    pub fn get(&self, key: &str) -> Option<&Be> {
        match self {
            Be::Dict(d) => d.get(key.as_bytes()),
            _ => None,
        }
    }

    pub fn int(&self) -> Option<i64> {
        match self {
            Be::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn str(&self) -> Option<String> {
        match self {
            Be::Str(s) => Some(String::from_utf8_lossy(s).into_owned()),
            _ => None,
        }
    }

    pub fn list(&self) -> Option<&[Be]> {
        match self {
            Be::List(l) => Some(l),
            _ => None,
        }
    }
}

struct Decoder<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Decoder<'_> {
    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    fn int_until(&mut self, end: u8) -> Option<i64> {
        let start = self.pos;
        while self.peek()? != end {
            self.pos += 1;
        }
        let s = std::str::from_utf8(&self.data[start..self.pos]).ok()?;
        self.pos += 1;
        s.parse().ok()
    }

    fn string(&mut self) -> Option<Vec<u8>> {
        let len = self.int_until(b':')?;
        if len < 0 {
            return None;
        }
        let len = len as usize;
        let end = self.pos.checked_add(len)?;
        let s = self.data.get(self.pos..end)?.to_vec();
        self.pos = end;
        Some(s)
    }

    fn value(&mut self, depth: usize) -> Option<Be> {
        if depth > 64 {
            return None;
        }
        match self.peek()? {
            b'i' => {
                self.pos += 1;
                self.int_until(b'e').map(Be::Int)
            }
            b'l' => {
                self.pos += 1;
                let mut list = Vec::new();
                while self.peek()? != b'e' {
                    list.push(self.value(depth + 1)?);
                }
                self.pos += 1;
                Some(Be::List(list))
            }
            b'd' => {
                self.pos += 1;
                let mut dict = BTreeMap::new();
                while self.peek()? != b'e' {
                    let key = self.string()?;
                    let value = self.value(depth + 1)?;
                    dict.insert(key, value);
                }
                self.pos += 1;
                Some(Be::Dict(dict))
            }
            b'0'..=b'9' => self.string().map(Be::Str),
            _ => None,
        }
    }
}

pub fn decode(data: &[u8]) -> Option<Be> {
    Decoder { data, pos: 0 }.value(0)
}

/// A parsed .torrent file
pub struct TorrentFile {
    #[allow(dead_code)]
    pub name: String,
    pub top_node: FileNode,
}

fn parse_nodes(info: &Be) -> Option<FileNode> {
    let files = info.get("files")?.list()?;
    let mut top = FileNode::top();
    for (i, file) in files.iter().enumerate() {
        // Unexpected format: throw away everything, file indexes need to be
        // correct.
        let length = file.get("length")?.int()?;
        let path: Vec<String> =
            file.get("path")?.list()?.iter().map(Be::str).collect::<Option<_>>()?;
        if path.is_empty() {
            return None;
        }
        top.insert(&path.join("/"), length, 0, i as i64, 1, 0);
    }
    Some(top)
}

pub fn parse_torrent_data(data: &[u8]) -> Option<TorrentFile> {
    let top = decode(data)?;
    let info = top.get("info")?;
    let name = info.get("name")?.str()?;

    let top_node = match parse_nodes(info) {
        Some(n) => n,
        None => {
            // single file mode
            let length = info.get("length")?.int()?;
            FileNode::file(&name, length, 0)
        }
    };

    Some(TorrentFile { name, top_node })
}

pub fn parse_torrent_file(filename: &str) -> Result<Option<TorrentFile>, String> {
    if !std::path::Path::new(filename).is_file() {
        gtk::glib::g_message!("trg", "{} does not exist", filename);
        return Ok(None);
    }
    let data = std::fs::read(filename).map_err(|e| e.to_string())?;
    Ok(parse_torrent_data(&data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        let data =
            b"d4:infod5:filesld6:lengthi5e4:pathl1:a1:beed6:lengthi7e4:pathl1:ceee4:name1:xee";
        let t = parse_torrent_data(data).unwrap();
        assert_eq!(t.name, "x");
        assert_eq!(t.top_node.children.len(), 2);
        let single = parse_torrent_data(b"d4:infod6:lengthi3e4:name1:yee").unwrap();
        assert_eq!(single.top_node.name.as_deref(), Some("y"));
        assert!(decode(b"l1:a").is_none());
    }
}
