// SPDX-License-Identifier: GPL-3.0-only

//! A read-only reader of SQLite database files (AUD-003, [ADR 0004]): as much
//! of the documented file format (<https://www.sqlite.org/fileformat2.html>)
//! as finding rows through an index and reading them needs: the header, the
//! schema, table and index b-trees, records and overflow pages. Nothing is
//! ever written, and no library is linked.
//!
//! The files come from the user: every read is bounded, and a malformed file
//! is an error, never a panic or an endless walk.
//!
//! [ADR 0004]: ../../../docs/adr/0004-local-audio-db.md

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Why a database cannot be read.
#[derive(Debug, thiserror::Error)]
pub enum SqliteError {
    /// The file cannot be read.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// Not an SQLite database.
    #[error("not an SQLite database")]
    NotSqlite,
    /// A part of the file is not what the format says.
    #[error("the database is damaged ({0})")]
    Corrupt(&'static str),
    /// Text in UTF-16, which this reader does not read.
    #[error("the database keeps its text in UTF-16")]
    Encoding,
}

type Result<T> = std::result::Result<T, SqliteError>;

/// A value of a record.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// NULL.
    Null,
    /// An integer.
    Integer(i64),
    /// A floating-point number.
    Real(f64),
    /// Text (UTF-8).
    Text(String),
    /// Bytes.
    Blob(Vec<u8>),
}

impl Value {
    /// The text, if it is text.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(t) => Some(t),
            _ => None,
        }
    }

    fn class(&self) -> u8 {
        match self {
            Value::Null => 0,
            Value::Integer(_) | Value::Real(_) => 1,
            Value::Text(_) => 2,
            Value::Blob(_) => 3,
        }
    }
}

/// SQLite's order of values with the BINARY collation: NULL, numbers, text
/// (byte by byte), blobs.
pub fn compare(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => x.cmp(y),
        (Value::Integer(x), Value::Real(y)) => (*x as f64).total_cmp(y),
        (Value::Real(x), Value::Integer(y)) => x.total_cmp(&(*y as f64)),
        (Value::Real(x), Value::Real(y)) => x.total_cmp(y),
        (Value::Text(x), Value::Text(y)) => x.as_bytes().cmp(y.as_bytes()),
        (Value::Blob(x), Value::Blob(y)) => x.cmp(y),
        _ => a.class().cmp(&b.class()),
    }
}

/// The largest record read: a blob larger is a damaged length.
const MAX_PAYLOAD: u64 = 64 << 20;

/// The deepest b-tree walked: SQLite's are a few levels deep.
const MAX_DEPTH: usize = 40;

/// Pages kept in memory.
const CACHE_PAGES: usize = 512;

/// A big-endian varint of SQLite at `at`: its value and its length.
fn varint(buf: &[u8], at: usize) -> Result<(u64, usize)> {
    let mut value: u64 = 0;
    for i in 0..9 {
        let b = *buf.get(at + i).ok_or(SqliteError::Corrupt("varint"))?;
        if i == 8 {
            return Ok(((value << 8) | u64::from(b), 9));
        }
        value = (value << 7) | u64::from(b & 0x7f);
        if b & 0x80 == 0 {
            return Ok((value, i + 1));
        }
    }
    Err(SqliteError::Corrupt("varint"))
}

fn u16_at(buf: &[u8], at: usize) -> Result<usize> {
    let b = buf.get(at..at + 2).ok_or(SqliteError::Corrupt("page"))?;
    Ok(usize::from(u16::from_be_bytes([b[0], b[1]])))
}

fn u32_at(buf: &[u8], at: usize) -> Result<u32> {
    let b = buf.get(at..at + 4).ok_or(SqliteError::Corrupt("page"))?;
    Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// The values of a record.
pub fn record(bytes: &[u8]) -> Result<Vec<Value>> {
    let (header_len, n) = varint(bytes, 0)?;
    let header_len = usize::try_from(header_len).map_err(|_| SqliteError::Corrupt("record"))?;
    if header_len > bytes.len() || header_len < n {
        return Err(SqliteError::Corrupt("record"));
    }
    let mut at = n;
    let mut body = header_len;
    let mut out = Vec::new();
    while at < header_len {
        let (serial, len) = varint(bytes, at)?;
        at += len;
        let take = |size: usize, body: &mut usize| -> Result<&[u8]> {
            let slice = bytes
                .get(*body..*body + size)
                .ok_or(SqliteError::Corrupt("record"))?;
            *body += size;
            Ok(slice)
        };
        let int = |b: &[u8]| -> i64 {
            // sign-extended big-endian
            let mut v: i64 = if b.first().is_some_and(|x| x & 0x80 != 0) {
                -1
            } else {
                0
            };
            for x in b {
                v = (v << 8) | i64::from(*x);
            }
            v
        };
        let value = match serial {
            0 => Value::Null,
            1 => Value::Integer(int(take(1, &mut body)?)),
            2 => Value::Integer(int(take(2, &mut body)?)),
            3 => Value::Integer(int(take(3, &mut body)?)),
            4 => Value::Integer(int(take(4, &mut body)?)),
            5 => Value::Integer(int(take(6, &mut body)?)),
            6 => Value::Integer(int(take(8, &mut body)?)),
            7 => {
                let b = take(8, &mut body)?;
                let mut a = [0u8; 8];
                a.copy_from_slice(b);
                Value::Real(f64::from_be_bytes(a))
            }
            8 => Value::Integer(0),
            9 => Value::Integer(1),
            10 | 11 => return Err(SqliteError::Corrupt("serial type")),
            n => {
                let size =
                    usize::try_from((n - 12) / 2).map_err(|_| SqliteError::Corrupt("record"))?;
                let b = take(size, &mut body)?;
                if n % 2 == 0 {
                    Value::Blob(b.to_vec())
                } else {
                    Value::Text(String::from_utf8_lossy(b).into_owned())
                }
            }
        };
        out.push(value);
    }
    Ok(out)
}

/// A row of the schema (`sqlite_schema`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaEntry {
    /// `table` or `index` (or `view`, `trigger`).
    pub kind: String,
    /// Its name.
    pub name: String,
    /// The table it belongs to.
    pub table: String,
    /// Its root page; 0 for views and triggers.
    pub root: u32,
    /// The SQL that made it.
    pub sql: String,
}

/// An open database, read-only.
#[derive(Debug)]
pub struct Database {
    file: File,
    page_size: usize,
    usable: usize,
    pages: u32,
    cache: Mutex<HashMap<u32, Arc<Vec<u8>>>>,
}

/// A b-tree page: its bytes, and where its header starts (100 on page 1).
struct Page {
    data: Arc<Vec<u8>>,
    head: usize,
}

impl Page {
    fn kind(&self) -> Result<u8> {
        self.data
            .get(self.head)
            .copied()
            .ok_or(SqliteError::Corrupt("page"))
    }

    fn interior(&self) -> Result<bool> {
        match self.kind()? {
            2 | 5 => Ok(true),
            10 | 13 => Ok(false),
            _ => Err(SqliteError::Corrupt("page type")),
        }
    }

    /// The offsets of its cells, in order.
    fn cells(&self) -> Result<Vec<usize>> {
        let count = u16_at(&self.data, self.head + 3)?;
        let array = self.head + if self.interior()? { 12 } else { 8 };
        (0..count)
            .map(|i| {
                let at = u16_at(&self.data, array + 2 * i)?;
                if at >= self.data.len() {
                    Err(SqliteError::Corrupt("cell"))
                } else {
                    Ok(at)
                }
            })
            .collect()
    }

    fn right(&self) -> Result<u32> {
        u32_at(&self.data, self.head + 8)
    }
}

/// How a scan goes on.
enum Walk {
    Go,
    Stop,
}

impl Database {
    /// Opens `path` read-only.
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let mut header = [0u8; 100];
        file.read_exact_at(&mut header, 0)
            .map_err(|_| SqliteError::NotSqlite)?;
        if &header[..16] != b"SQLite format 3\0" {
            return Err(SqliteError::NotSqlite);
        }
        let page_size = match u16_at(&header, 16)? {
            1 => 65_536,
            n if (512..=32_768).contains(&n) && n.is_power_of_two() => n,
            _ => return Err(SqliteError::Corrupt("page size")),
        };
        let usable = page_size - usize::from(header[20]);
        if usable < 480 {
            return Err(SqliteError::Corrupt("reserved space"));
        }
        if !matches!(u32_at(&header, 56)?, 0 | 1) {
            return Err(SqliteError::Encoding);
        }
        let length = file.metadata()?.len();
        let pages =
            u32::try_from(length / page_size as u64).map_err(|_| SqliteError::Corrupt("size"))?;
        Ok(Self {
            file,
            page_size,
            usable,
            pages,
            cache: Mutex::new(HashMap::new()),
        })
    }

    fn page_bytes(&self, n: u32) -> Result<Arc<Vec<u8>>> {
        if n == 0 || n > self.pages {
            return Err(SqliteError::Corrupt("page number"));
        }
        if let Some(p) = self.cache.lock().unwrap_or_else(|e| e.into_inner()).get(&n) {
            return Ok(Arc::clone(p));
        }
        let mut data = vec![0u8; self.page_size];
        self.file
            .read_exact_at(&mut data, u64::from(n - 1) * self.page_size as u64)?;
        let data = Arc::new(data);
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if cache.len() >= CACHE_PAGES {
            cache.clear();
        }
        cache.insert(n, Arc::clone(&data));
        Ok(data)
    }

    fn page(&self, n: u32) -> Result<Page> {
        Ok(Page {
            data: self.page_bytes(n)?,
            head: if n == 1 { 100 } else { 0 },
        })
    }

    /// The payload of a cell that starts at `at` with `total` bytes, with its
    /// overflow pages.
    fn payload(&self, page: &Page, at: usize, total: u64, table: bool) -> Result<Vec<u8>> {
        if total > MAX_PAYLOAD {
            return Err(SqliteError::Corrupt("payload size"));
        }
        let total = total as usize;
        let u = self.usable;
        let x = if table {
            u - 35
        } else {
            (u - 12) * 64 / 255 - 23
        };
        let m = (u - 12) * 32 / 255 - 23;
        let local = if total <= x {
            total
        } else {
            let k = m + (total - m) % (u - 4);
            if k <= x { k } else { m }
        };
        let mut out = page
            .data
            .get(at..at + local)
            .ok_or(SqliteError::Corrupt("cell payload"))?
            .to_vec();
        if local == total {
            return Ok(out);
        }
        let mut next = u32_at(&page.data, at + local)?;
        // at most as many pages as the payload needs: a loop ends there
        let mut budget = (total - local) / (u - 4) + 1;
        while out.len() < total {
            if next == 0 || budget == 0 {
                return Err(SqliteError::Corrupt("overflow chain"));
            }
            budget -= 1;
            let data = self.page_bytes(next)?;
            let take = (total - out.len()).min(u - 4);
            out.extend_from_slice(
                data.get(4..4 + take)
                    .ok_or(SqliteError::Corrupt("overflow"))?,
            );
            next = u32_at(&data, 0)?;
        }
        Ok(out)
    }

    /// The rowid and record of a table leaf cell.
    fn table_cell(&self, page: &Page, at: usize) -> Result<(i64, Vec<Value>)> {
        let (size, a) = varint(&page.data, at)?;
        let (rowid, b) = varint(&page.data, at + a)?;
        let bytes = self.payload(page, at + a + b, size, true)?;
        Ok((rowid as i64, record(&bytes)?))
    }

    /// The record of an index cell, and its left child if interior.
    fn index_cell(
        &self,
        page: &Page,
        at: usize,
        interior: bool,
    ) -> Result<(Option<u32>, Vec<Value>)> {
        let (left, at) = if interior {
            (Some(u32_at(&page.data, at)?), at + 4)
        } else {
            (None, at)
        };
        let (size, n) = varint(&page.data, at)?;
        let bytes = self.payload(page, at + n, size, false)?;
        Ok((left, record(&bytes)?))
    }

    /// Visits every row of the table at `root`, in rowid order, until
    /// `visit` stops.
    pub fn scan(&self, root: u32, visit: &mut dyn FnMut(i64, Vec<Value>) -> bool) -> Result<()> {
        let mut budget = self.pages as usize;
        self.scan_page(root, 0, &mut budget, visit).map(drop)
    }

    fn scan_page(
        &self,
        n: u32,
        depth: usize,
        budget: &mut usize,
        visit: &mut dyn FnMut(i64, Vec<Value>) -> bool,
    ) -> Result<Walk> {
        if depth > MAX_DEPTH || *budget == 0 {
            return Err(SqliteError::Corrupt("b-tree depth"));
        }
        *budget -= 1;
        let page = self.page(n)?;
        match page.kind()? {
            13 => {
                for at in page.cells()? {
                    let (rowid, values) = self.table_cell(&page, at)?;
                    if !visit(rowid, values) {
                        return Ok(Walk::Stop);
                    }
                }
                Ok(Walk::Go)
            }
            5 => {
                for at in page.cells()? {
                    let left = u32_at(&page.data, at)?;
                    if let Walk::Stop = self.scan_page(left, depth + 1, budget, visit)? {
                        return Ok(Walk::Stop);
                    }
                }
                self.scan_page(page.right()?, depth + 1, budget, visit)
            }
            _ => Err(SqliteError::Corrupt("table page type")),
        }
    }

    /// The row `rowid` of the table at `root`.
    pub fn row(&self, root: u32, rowid: i64) -> Result<Option<Vec<Value>>> {
        let mut n = root;
        for _ in 0..MAX_DEPTH {
            let page = self.page(n)?;
            match page.kind()? {
                13 => {
                    for at in page.cells()? {
                        let (size, a) = varint(&page.data, at)?;
                        let (id, b) = varint(&page.data, at + a)?;
                        if id as i64 == rowid {
                            let bytes = self.payload(&page, at + a + b, size, true)?;
                            return Ok(Some(record(&bytes)?));
                        }
                    }
                    return Ok(None);
                }
                5 => {
                    let mut child = page.right()?;
                    for at in page.cells()? {
                        let left = u32_at(&page.data, at)?;
                        let (key, _) = varint(&page.data, at + 4)?;
                        if rowid <= key as i64 {
                            child = left;
                            break;
                        }
                    }
                    n = child;
                }
                _ => return Err(SqliteError::Corrupt("table page type")),
            }
        }
        Err(SqliteError::Corrupt("b-tree depth"))
    }

    /// The records of the index at `root` whose first columns equal
    /// `prefix`, in index order, at most `limit`.
    pub fn index_find(&self, root: u32, prefix: &[Value], limit: usize) -> Result<Vec<Vec<Value>>> {
        let mut out = Vec::new();
        let mut budget = self.pages as usize;
        self.index_page(root, prefix, limit, 0, &mut budget, &mut out)?;
        Ok(out)
    }

    fn index_page(
        &self,
        n: u32,
        prefix: &[Value],
        limit: usize,
        depth: usize,
        budget: &mut usize,
        out: &mut Vec<Vec<Value>>,
    ) -> Result<Walk> {
        if depth > MAX_DEPTH || *budget == 0 {
            return Err(SqliteError::Corrupt("b-tree depth"));
        }
        *budget -= 1;
        let page = self.page(n)?;
        let interior = match page.kind()? {
            2 => true,
            10 => false,
            _ => return Err(SqliteError::Corrupt("index page type")),
        };
        let against = |values: &[Value]| -> Ordering {
            for (i, p) in prefix.iter().enumerate() {
                let v = values.get(i).unwrap_or(&Value::Null);
                match compare(v, p) {
                    Ordering::Equal => {}
                    other => return other,
                }
            }
            Ordering::Equal
        };
        for at in page.cells()? {
            let (left, values) = self.index_cell(&page, at, interior)?;
            let order = against(&values);
            if order == Ordering::Less {
                continue;
            }
            if let Some(left) = left
                && let Walk::Stop = self.index_page(left, prefix, limit, depth + 1, budget, out)?
            {
                return Ok(Walk::Stop);
            }
            if order == Ordering::Greater || out.len() >= limit {
                return Ok(Walk::Stop);
            }
            out.push(values);
        }
        if interior {
            return self.index_page(page.right()?, prefix, limit, depth + 1, budget, out);
        }
        Ok(Walk::Go)
    }

    /// The schema: the tables and indexes.
    pub fn schema(&self) -> Result<Vec<SchemaEntry>> {
        let mut out = Vec::new();
        self.scan(1, &mut |_, v| {
            let text = |i: usize| {
                v.get(i)
                    .and_then(Value::as_text)
                    .unwrap_or_default()
                    .to_string()
            };
            let root = match v.get(3) {
                Some(Value::Integer(r)) => u32::try_from(*r).unwrap_or(0),
                _ => 0,
            };
            out.push(SchemaEntry {
                kind: text(0),
                name: text(1),
                table: text(2),
                root,
                sql: text(4),
            });
            true
        })?;
        Ok(out)
    }
}

/// The names in the parentheses of a `CREATE TABLE` or `CREATE INDEX`, in
/// order, without quotes; for a table, `(column, is the rowid)`. Table
/// constraints (`PRIMARY KEY (…)`, `UNIQUE`, …) are left out.
pub fn columns(sql: &str) -> Vec<(String, bool)> {
    let (Some(open), Some(close)) = (sql.find('('), sql.rfind(')')) else {
        return Vec::new();
    };
    if close <= open {
        return Vec::new();
    }
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = open + 1;
    for (i, c) in sql[open + 1..close].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&sql[start..open + 1 + i]);
                start = open + 2 + i;
            }
            _ => {}
        }
    }
    parts.push(&sql[start..close]);
    parts
        .into_iter()
        .filter_map(|part| {
            let part = part.trim();
            let upper = part.to_uppercase();
            let constraint = ["PRIMARY ", "UNIQUE", "CHECK", "FOREIGN ", "CONSTRAINT "]
                .iter()
                .any(|k| upper.starts_with(k));
            if constraint || part.is_empty() {
                return None;
            }
            let name: String = match part.chars().next() {
                Some(q @ ('"' | '`' | '[' | '\'')) => {
                    let end = if q == '[' { ']' } else { q };
                    part[1..].chars().take_while(|c| *c != end).collect()
                }
                _ => part
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_string(),
            };
            let words: Vec<&str> = upper.split_whitespace().collect();
            let rowid = words.get(1) == Some(&"INTEGER")
                && upper.contains("PRIMARY KEY")
                && !upper.contains(" DESC");
            Some((name, rowid))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_and_records() {
        assert_eq!(varint(&[0x05], 0).unwrap(), (5, 1));
        assert_eq!(varint(&[0x81, 0x00], 0).unwrap(), (128, 2));
        assert_eq!(varint(&[0xff; 9], 0).unwrap().1, 9);
        assert!(varint(&[0x81], 0).is_err());
        // header: 4 bytes (len, int8, text 2, null); body: 7, "ab"
        let r = record(&[4, 1, 17, 0, 7, b'a', b'b']).unwrap();
        assert_eq!(
            r,
            [Value::Integer(7), Value::Text("ab".into()), Value::Null]
        );
        assert_eq!(
            record(&[3, 2, 0, 0xff, 0xfe]).unwrap(),
            [Value::Integer(-2), Value::Null]
        );
        // a length past the end, a reserved type
        assert!(record(&[2, 21, b'a']).is_err());
        assert!(record(&[2, 10]).is_err());
        assert_eq!(
            compare(&Value::Text("a".into()), &Value::Integer(9)),
            Ordering::Greater
        );
        assert_eq!(
            compare(&Value::Integer(2), &Value::Real(2.5)),
            Ordering::Less
        );
    }

    #[test]
    fn the_columns_of_the_sql() {
        let t = columns(
            "CREATE TABLE entries (id INTEGER PRIMARY KEY, \"expression\" TEXT NOT NULL, reading TEXT, \
             [file] TEXT, CHECK (length(file) > 0), PRIMARY KEY (x))",
        );
        let names: Vec<(&str, bool)> = t.iter().map(|(n, r)| (n.as_str(), *r)).collect();
        assert_eq!(
            names,
            [
                ("id", true),
                ("expression", false),
                ("reading", false),
                ("file", false)
            ]
        );
        let i = columns(
            "CREATE INDEX idx_expr_reading ON entries (expression, reading COLLATE BINARY)",
        );
        let names: Vec<&str> = i.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["expression", "reading"]);
        assert!(columns("CREATE TABLE x").is_empty());
    }
}
