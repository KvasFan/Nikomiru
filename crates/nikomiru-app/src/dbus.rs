// SPDX-License-Identifier: GPL-3.0-only

//! A minimal D-Bus client for the desktop portals (PLY-013). It is written here
//! instead of using `zbus`, which would add about 45 crates for three fixed
//! calls. It covers what Nikomiru needs and nothing more: `EXTERNAL`
//! authentication, method calls and their replies, and the marshalling of the
//! basic types, strings, object paths, signatures, variants, arrays, structs and
//! dict entries. Messages are written little-endian and read in both byte
//! orders; file descriptor passing is not supported.

use std::io::{self, Read, Write};
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::{SocketAddr, UnixStream};
use std::time::Duration;

/// Largest message accepted (header and body). The D-Bus limit is 128 MiB;
/// portal replies are a few hundred bytes.
const MAX_MESSAGE: usize = 1 << 20;

/// Deepest nesting of containers accepted (the D-Bus limit is 64).
const MAX_DEPTH: usize = 64;

/// How long a call waits for its reply.
const CALL_TIMEOUT: Duration = Duration::from_secs(10);

/// A D-Bus value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `y`
    Byte(u8),
    /// `b`
    Bool(bool),
    /// `n`
    I16(i16),
    /// `q`
    U16(u16),
    /// `i`
    I32(i32),
    /// `u`
    U32(u32),
    /// `x`
    I64(i64),
    /// `t`
    U64(u64),
    /// `d`
    F64(f64),
    /// `h` (an index into the passed descriptors; never sent by Nikomiru)
    Fd(u32),
    /// `s`
    Str(String),
    /// `o`
    Path(String),
    /// `g`
    Sig(String),
    /// `v`
    Variant(Box<Value>),
    /// `a…`: the element signature and the elements.
    Array(String, Vec<Value>),
    /// `(…)`
    Struct(Vec<Value>),
    /// `{kv}`, only inside an array.
    Entry(Box<Value>, Box<Value>),
}

impl Value {
    /// The value's signature.
    pub fn signature(&self) -> String {
        match self {
            Value::Byte(_) => "y".into(),
            Value::Bool(_) => "b".into(),
            Value::I16(_) => "n".into(),
            Value::U16(_) => "q".into(),
            Value::I32(_) => "i".into(),
            Value::U32(_) => "u".into(),
            Value::I64(_) => "x".into(),
            Value::U64(_) => "t".into(),
            Value::F64(_) => "d".into(),
            Value::Fd(_) => "h".into(),
            Value::Str(_) => "s".into(),
            Value::Path(_) => "o".into(),
            Value::Sig(_) => "g".into(),
            Value::Variant(_) => "v".into(),
            Value::Array(element, _) => format!("a{element}"),
            Value::Struct(fields) => {
                format!(
                    "({})",
                    fields.iter().map(Value::signature).collect::<String>()
                )
            }
            Value::Entry(k, v) => format!("{{{}{}}}", k.signature(), v.signature()),
        }
    }

    /// A string-keyed dictionary of variants (`a{sv}`).
    pub fn dict(entries: &[(&str, Value)]) -> Value {
        Value::Array(
            "{sv}".into(),
            entries
                .iter()
                .map(|(k, v)| {
                    Value::Entry(
                        Box::new(Value::Str((*k).into())),
                        Box::new(Value::Variant(Box::new(v.clone()))),
                    )
                })
                .collect(),
        )
    }
}

fn invalid(what: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.into())
}

/// Alignment of the type that starts `sig`.
fn alignment(sig: &str) -> usize {
    match sig.as_bytes().first() {
        Some(b'n' | b'q') => 2,
        Some(b'b' | b'i' | b'u' | b'h' | b's' | b'o' | b'a') => 4,
        Some(b'x' | b't' | b'd' | b'(' | b'{') => 8,
        _ => 1,
    }
}

/// Splits the first complete type off `sig`.
fn split_type(sig: &str) -> io::Result<(&str, &str)> {
    let bytes = sig.as_bytes();
    let len = match bytes.first() {
        None => return Err(invalid("empty signature")),
        Some(b'a') => 1 + split_type(&sig[1..])?.0.len(),
        Some(&open @ (b'(' | b'{')) => {
            let close = if open == b'(' { b')' } else { b'}' };
            let mut depth = 0usize;
            let mut end = None;
            for (i, &c) in bytes.iter().enumerate() {
                if c == open {
                    depth += 1;
                } else if c == close {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i + 1);
                        break;
                    }
                }
            }
            end.ok_or_else(|| invalid("unbalanced signature"))?
        }
        Some(_) => 1,
    };
    Ok(sig.split_at(len))
}

/// The complete types of a signature, in order.
fn split_types(mut sig: &str) -> io::Result<Vec<&str>> {
    let mut out = Vec::new();
    while !sig.is_empty() {
        let (first, rest) = split_type(sig)?;
        out.push(first);
        sig = rest;
    }
    Ok(out)
}

/// Marshals values little-endian; offsets are relative to the message start.
struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn pad(&mut self, align: usize) {
        while !self.buf.len().is_multiple_of(align) {
            self.buf.push(0);
        }
    }

    fn u32(&mut self, v: u32) {
        self.pad(4);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn string(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.buf.extend_from_slice(s.as_bytes());
        self.buf.push(0);
    }

    fn write(&mut self, v: &Value) {
        match v {
            Value::Byte(b) => self.buf.push(*b),
            Value::Bool(b) => self.u32(u32::from(*b)),
            Value::I16(n) => {
                self.pad(2);
                self.buf.extend_from_slice(&n.to_le_bytes());
            }
            Value::U16(n) => {
                self.pad(2);
                self.buf.extend_from_slice(&n.to_le_bytes());
            }
            Value::I32(n) => {
                self.pad(4);
                self.buf.extend_from_slice(&n.to_le_bytes());
            }
            Value::U32(n) | Value::Fd(n) => self.u32(*n),
            Value::I64(n) => {
                self.pad(8);
                self.buf.extend_from_slice(&n.to_le_bytes());
            }
            Value::U64(n) => {
                self.pad(8);
                self.buf.extend_from_slice(&n.to_le_bytes());
            }
            Value::F64(n) => {
                self.pad(8);
                self.buf.extend_from_slice(&n.to_le_bytes());
            }
            Value::Str(s) | Value::Path(s) => self.string(s),
            Value::Sig(s) => {
                self.buf.push(s.len() as u8);
                self.buf.extend_from_slice(s.as_bytes());
                self.buf.push(0);
            }
            Value::Variant(inner) => {
                self.write(&Value::Sig(inner.signature()));
                self.write(inner);
            }
            Value::Array(element, items) => {
                self.u32(0);
                let at = self.buf.len() - 4;
                self.pad(alignment(element));
                let start = self.buf.len();
                for item in items {
                    self.write(item);
                }
                let len = (self.buf.len() - start) as u32;
                self.buf[at..at + 4].copy_from_slice(&len.to_le_bytes());
            }
            Value::Struct(fields) => {
                self.pad(8);
                for f in fields {
                    self.write(f);
                }
            }
            Value::Entry(k, v) => {
                self.pad(8);
                self.write(k);
                self.write(v);
            }
        }
    }
}

/// Unmarshals values; `pos` is relative to the message start.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    big_endian: bool,
}

impl<'a> Reader<'a> {
    fn align(&mut self, align: usize) -> io::Result<()> {
        let next = self.pos.next_multiple_of(align);
        if next > self.buf.len() {
            return Err(invalid("truncated message"));
        }
        self.pos = next;
        Ok(())
    }

    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.buf.len())
            .ok_or_else(|| invalid("truncated message"))?;
        let out = &self.buf[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn fixed<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        self.align(N)?;
        let mut out = [0; N];
        out.copy_from_slice(self.take(N)?);
        if self.big_endian {
            out.reverse();
        }
        Ok(out)
    }

    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.fixed()?))
    }

    fn text(&mut self, len: usize) -> io::Result<String> {
        let bytes = self.take(len)?;
        if self.take(1)? != [0] {
            return Err(invalid("string without its NUL"));
        }
        String::from_utf8(bytes.to_vec()).map_err(|_| invalid("string is not UTF-8"))
    }

    fn read(&mut self, ty: &str, depth: usize) -> io::Result<Value> {
        if depth > MAX_DEPTH {
            return Err(invalid("values nested too deeply"));
        }
        Ok(match ty.as_bytes().first() {
            Some(b'y') => Value::Byte(self.take(1)?[0]),
            Some(b'b') => Value::Bool(self.u32()? != 0),
            Some(b'n') => Value::I16(i16::from_le_bytes(self.fixed()?)),
            Some(b'q') => Value::U16(u16::from_le_bytes(self.fixed()?)),
            Some(b'i') => Value::I32(i32::from_le_bytes(self.fixed()?)),
            Some(b'u') => Value::U32(self.u32()?),
            Some(b'h') => Value::Fd(self.u32()?),
            Some(b'x') => Value::I64(i64::from_le_bytes(self.fixed()?)),
            Some(b't') => Value::U64(u64::from_le_bytes(self.fixed()?)),
            Some(b'd') => Value::F64(f64::from_le_bytes(self.fixed()?)),
            Some(b's') => {
                let len = self.u32()? as usize;
                Value::Str(self.text(len)?)
            }
            Some(b'o') => {
                let len = self.u32()? as usize;
                Value::Path(self.text(len)?)
            }
            Some(b'g') => {
                let len = usize::from(self.take(1)?[0]);
                Value::Sig(self.text(len)?)
            }
            Some(b'v') => {
                let Value::Sig(sig) = self.read("g", depth)? else {
                    return Err(invalid("variant without a signature"));
                };
                let (inner, rest) = split_type(&sig)?;
                if !rest.is_empty() {
                    return Err(invalid("variant with more than one type"));
                }
                Value::Variant(Box::new(self.read(inner, depth + 1)?))
            }
            Some(b'a') => {
                let element = &ty[1..];
                let len = self.u32()? as usize;
                self.align(alignment(element))?;
                let end = self
                    .pos
                    .checked_add(len)
                    .filter(|&e| e <= self.buf.len())
                    .ok_or_else(|| invalid("array longer than the message"))?;
                let mut items = Vec::new();
                while self.pos < end {
                    items.push(self.read(element, depth + 1)?);
                }
                if self.pos != end {
                    return Err(invalid("array length mismatch"));
                }
                Value::Array(element.into(), items)
            }
            Some(b'(') => {
                self.align(8)?;
                let mut fields = Vec::new();
                for f in split_types(&ty[1..ty.len() - 1])? {
                    fields.push(self.read(f, depth + 1)?);
                }
                Value::Struct(fields)
            }
            Some(b'{') => {
                self.align(8)?;
                let inner = split_types(&ty[1..ty.len() - 1])?;
                let [k, v] = inner[..] else {
                    return Err(invalid("dict entry without two types"));
                };
                Value::Entry(
                    Box::new(self.read(k, depth + 1)?),
                    Box::new(self.read(v, depth + 1)?),
                )
            }
            _ => return Err(invalid(format!("unsupported type {ty}"))),
        })
    }
}

/// Kinds of message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A method call.
    MethodCall = 1,
    /// A successful reply.
    MethodReturn = 2,
    /// An error reply.
    Error = 3,
    /// A signal.
    Signal = 4,
}

/// A D-Bus message.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// What the message is.
    pub kind: Kind,
    /// Serial number, set when sent.
    pub serial: u32,
    /// Object path.
    pub path: Option<String>,
    /// Interface.
    pub interface: Option<String>,
    /// Method or signal name.
    pub member: Option<String>,
    /// Error name, for errors.
    pub error_name: Option<String>,
    /// The call this message answers.
    pub reply_serial: Option<u32>,
    /// Destination bus name.
    pub destination: Option<String>,
    /// Sender bus name (set by the bus).
    pub sender: Option<String>,
    /// The body.
    pub body: Vec<Value>,
}

impl Message {
    fn empty(kind: Kind) -> Self {
        Self {
            kind,
            serial: 0,
            path: None,
            interface: None,
            member: None,
            error_name: None,
            reply_serial: None,
            destination: None,
            sender: None,
            body: Vec::new(),
        }
    }

    /// A method call.
    pub fn call(destination: &str, path: &str, interface: &str, member: &str) -> Self {
        Self {
            path: Some(path.into()),
            interface: Some(interface.into()),
            member: Some(member.into()),
            destination: Some(destination.into()),
            ..Self::empty(Kind::MethodCall)
        }
    }

    /// A signal.
    pub fn signal(path: &str, interface: &str, member: &str) -> Self {
        Self {
            path: Some(path.into()),
            interface: Some(interface.into()),
            member: Some(member.into()),
            ..Self::empty(Kind::Signal)
        }
    }

    /// The successful reply to `call`.
    pub fn reply_to(call: &Message, body: Vec<Value>) -> Self {
        Self {
            reply_serial: Some(call.serial),
            destination: call.sender.clone(),
            body,
            ..Self::empty(Kind::MethodReturn)
        }
    }

    /// The same message with `body`.
    pub fn with_body(mut self, body: Vec<Value>) -> Self {
        self.body = body;
        self
    }

    /// The text of an error reply, if the body has one.
    fn error_text(&self) -> String {
        let name = self.error_name.as_deref().unwrap_or("error");
        match self.body.first() {
            Some(Value::Str(text)) => format!("{name}: {text}"),
            _ => name.to_string(),
        }
    }

    /// The wire form, with `serial`.
    pub fn encode(&self, serial: u32) -> Vec<u8> {
        let mut body = Writer { buf: Vec::new() };
        for v in &self.body {
            body.write(v);
        }
        let signature: String = self.body.iter().map(Value::signature).collect();
        let mut fields = Vec::new();
        let mut field = |code: u8, v: Value| {
            fields.push(Value::Struct(vec![
                Value::Byte(code),
                Value::Variant(Box::new(v)),
            ]));
        };
        if let Some(p) = &self.path {
            field(1, Value::Path(p.clone()));
        }
        if let Some(i) = &self.interface {
            field(2, Value::Str(i.clone()));
        }
        if let Some(m) = &self.member {
            field(3, Value::Str(m.clone()));
        }
        if let Some(e) = &self.error_name {
            field(4, Value::Str(e.clone()));
        }
        if let Some(r) = self.reply_serial {
            field(5, Value::U32(r));
        }
        if let Some(d) = &self.destination {
            field(6, Value::Str(d.clone()));
        }
        if let Some(s) = &self.sender {
            field(7, Value::Str(s.clone()));
        }
        if !signature.is_empty() {
            field(8, Value::Sig(signature));
        }
        let mut w = Writer {
            buf: vec![b'l', self.kind as u8, 0, 1],
        };
        w.u32(body.buf.len() as u32);
        w.u32(serial);
        w.write(&Value::Array("(yv)".into(), fields));
        w.pad(8);
        w.buf.extend_from_slice(&body.buf);
        w.buf
    }

    /// Reads one message from `stream`.
    pub fn decode(stream: &mut impl Read) -> io::Result<Message> {
        let mut fixed = [0u8; 16];
        stream.read_exact(&mut fixed)?;
        let big_endian = match fixed[0] {
            b'l' => false,
            b'B' => true,
            _ => return Err(invalid("unknown byte order")),
        };
        let word = |at: usize| {
            let mut b = [fixed[at], fixed[at + 1], fixed[at + 2], fixed[at + 3]];
            if big_endian {
                b.reverse();
            }
            u32::from_le_bytes(b) as usize
        };
        let (body_len, fields_len) = (word(4), word(12));
        let header_len = (16 + fields_len).next_multiple_of(8);
        if header_len + body_len > MAX_MESSAGE {
            return Err(invalid("message too large"));
        }
        let mut buf = vec![0u8; header_len + body_len];
        buf[..16].copy_from_slice(&fixed);
        stream.read_exact(&mut buf[16..])?;

        let kind = match fixed[1] {
            1 => Kind::MethodCall,
            2 => Kind::MethodReturn,
            3 => Kind::Error,
            4 => Kind::Signal,
            _ => return Err(invalid("unknown message type")),
        };
        let mut msg = Message::empty(kind);
        msg.serial = word(8) as u32;
        let mut header = Reader {
            buf: &buf[..16 + fields_len],
            pos: 12,
            big_endian,
        };
        let Value::Array(_, fields) = header.read("a(yv)", 0)? else {
            return Err(invalid("bad header fields"));
        };
        let mut signature = String::new();
        for f in fields {
            let Value::Struct(f) = f else { continue };
            let (Some(Value::Byte(code)), Some(Value::Variant(v))) = (f.first(), f.get(1)) else {
                continue;
            };
            match (code, v.as_ref()) {
                (1, Value::Path(p)) => msg.path = Some(p.clone()),
                (2, Value::Str(s)) => msg.interface = Some(s.clone()),
                (3, Value::Str(s)) => msg.member = Some(s.clone()),
                (4, Value::Str(s)) => msg.error_name = Some(s.clone()),
                (5, Value::U32(r)) => msg.reply_serial = Some(*r),
                (6, Value::Str(s)) => msg.destination = Some(s.clone()),
                (7, Value::Str(s)) => msg.sender = Some(s.clone()),
                (8, Value::Sig(s)) => signature = s.clone(),
                _ => {}
            }
        }
        // the body starts 8-aligned, so offsets relative to it align the same way
        let mut body = Reader {
            buf: &buf[header_len..],
            pos: 0,
            big_endian,
        };
        for ty in split_types(&signature)? {
            msg.body.push(body.read(ty, 0)?);
        }
        Ok(msg)
    }
}

/// Where a D-Bus server listens, from an address like `unix:path=/run/user/1000/bus`.
pub fn parse_address(address: &str) -> io::Result<SocketAddr> {
    for entry in address.split(';') {
        let Some(params) = entry.strip_prefix("unix:") else {
            continue;
        };
        for param in params.split(',') {
            let Some((key, value)) = param.split_once('=') else {
                continue;
            };
            let value = unescape(value)?;
            match key {
                "path" => return SocketAddr::from_pathname(value),
                "abstract" => {
                    use std::os::linux::net::SocketAddrExt;
                    return SocketAddr::from_abstract_name(value.as_bytes());
                }
                _ => {}
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("no usable unix address in {address}"),
    ))
}

/// Undoes the `%xx` escapes of a D-Bus address value.
fn unescape(value: &str) -> io::Result<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = value
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .ok_or_else(|| invalid("bad escape in the bus address"))?;
            out.push(hex);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| invalid("bus address is not UTF-8"))
}

/// A connection to a message bus.
#[derive(Debug)]
pub struct Connection {
    stream: UnixStream,
    serial: u32,
    unique_name: String,
}

impl Connection {
    /// Connects to the session bus (`DBUS_SESSION_BUS_ADDRESS`).
    pub fn session() -> io::Result<Self> {
        let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").map_err(|_| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "DBUS_SESSION_BUS_ADDRESS is not set",
            )
        })?;
        Self::open(&address)
    }

    /// Connects to the bus at `address`, authenticates and says `Hello`.
    pub fn open(address: &str) -> io::Result<Self> {
        let stream = UnixStream::connect_addr(&parse_address(address)?)?;
        stream.set_read_timeout(Some(CALL_TIMEOUT))?;
        stream.set_write_timeout(Some(CALL_TIMEOUT))?;
        let mut conn = Self {
            stream,
            serial: 0,
            unique_name: String::new(),
        };
        conn.authenticate()?;
        let reply = conn.call(Message::call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "Hello",
        ))?;
        if let Some(Value::Str(name)) = reply.body.first() {
            conn.unique_name = name.clone();
        }
        Ok(conn)
    }

    /// The name the bus gave this connection.
    pub fn unique_name(&self) -> &str {
        &self.unique_name
    }

    /// How long [`Self::receive`] waits for a message; `None` waits for as
    /// long as it takes (a portal dialog waits for the user).
    pub fn set_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.stream.set_read_timeout(timeout)
    }

    fn authenticate(&mut self) -> io::Result<()> {
        let uid = std::fs::metadata("/proc/self")?.uid();
        let hex: String = uid
            .to_string()
            .bytes()
            .map(|b| format!("{b:02x}"))
            .collect();
        self.stream
            .write_all(format!("\0AUTH EXTERNAL {hex}\r\n").as_bytes())?;
        // read the answer a byte at a time: binary messages follow it
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        while !line.ends_with(b"\r\n") {
            if line.len() > 512 {
                return Err(invalid("authentication answer too long"));
            }
            self.stream.read_exact(&mut byte)?;
            line.push(byte[0]);
        }
        if !line.starts_with(b"OK ") {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the bus refused EXTERNAL authentication",
            ));
        }
        self.stream.write_all(b"BEGIN\r\n")
    }

    /// Sends `msg` and returns its serial.
    pub fn send(&mut self, msg: &Message) -> io::Result<u32> {
        self.serial = self.serial.wrapping_add(1).max(1);
        self.stream.write_all(&msg.encode(self.serial))?;
        Ok(self.serial)
    }

    /// The next message from the bus.
    pub fn receive(&mut self) -> io::Result<Message> {
        Message::decode(&mut self.stream)
    }

    /// Calls a method and waits for its reply, skipping other messages. An
    /// error reply becomes an `Err` with the error's name and text.
    pub fn call(&mut self, msg: Message) -> io::Result<Message> {
        let serial = self.send(&msg)?;
        loop {
            let reply = self.receive()?;
            if reply.reply_serial != Some(serial) {
                continue;
            }
            return match reply.kind {
                Kind::Error => Err(io::Error::other(reply.error_text())),
                _ => Ok(reply),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_split_into_complete_types() {
        assert_eq!(split_types("sua{sv}").unwrap(), ["s", "u", "a{sv}"]);
        assert_eq!(split_types("a(yv)o").unwrap(), ["a(yv)", "o"]);
        assert_eq!(split_types("(s(ii))b").unwrap(), ["(s(ii))", "b"]);
        assert!(split_types("(si").is_err());
    }

    #[test]
    fn a_message_survives_a_round_trip() {
        let mut call = Message::call(
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Inhibit",
            "Inhibit",
        )
        .with_body(vec![
            Value::Str(String::new()),
            Value::U32(8),
            Value::dict(&[
                ("reason", Value::Str("Playing a video".into())),
                (
                    "n",
                    Value::Struct(vec![Value::Byte(1), Value::U64(u64::MAX)]),
                ),
            ]),
        ]);
        call.sender = Some(":1.42".into());
        let wire = call.encode(7);
        let back = Message::decode(&mut wire.as_slice()).unwrap();
        assert_eq!(back.serial, 7);
        assert_eq!(Message { serial: 0, ..back }, call);
    }

    #[test]
    fn the_wire_format_matches_the_specification() {
        // Hello, as dbus-daemon expects it (checked byte by byte against the
        // D-Bus specification's marshalling rules)
        let hello = Message::call(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "Hello",
        )
        .encode(1);
        assert_eq!(&hello[..4], b"l\x01\x00\x01");
        assert_eq!(&hello[4..8], &0u32.to_le_bytes(), "empty body");
        assert_eq!(&hello[8..12], &1u32.to_le_bytes(), "serial");
        // first header field: code 1 (path), variant signature "o", the path
        assert_eq!(&hello[16..20], b"\x01\x01o\x00");
        assert_eq!(&hello[20..24], &21u32.to_le_bytes());
        assert_eq!(&hello[24..46], b"/org/freedesktop/DBus\0");
        assert_eq!(hello.len() % 8, 0, "no body: the header ends 8-aligned");
    }

    #[test]
    fn malformed_input_is_an_error_not_a_panic() {
        let good = Message::call("a.b", "/a", "a.b", "C")
            .with_body(vec![Value::Str("x".into())])
            .encode(3);
        for cut in 0..good.len() {
            assert!(Message::decode(&mut &good[..cut]).is_err());
        }
        let mut huge = good.clone();
        huge[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Message::decode(&mut huge.as_slice()).is_err());
        // 80 variants, each holding the next one: deeper than MAX_DEPTH
        let nested: Vec<u8> = [1, b'v', 0].repeat(80);
        let mut deep = Reader {
            buf: &nested,
            pos: 0,
            big_endian: false,
        };
        let err = deep.read("v", 0).unwrap_err();
        assert!(err.to_string().contains("deeply"), "{err}");
    }

    #[test]
    fn big_endian_messages_are_read() {
        // a big-endian method return with body "u" = 0x01020304, reply serial 9
        let mut m = vec![b'B', 2, 0, 1, 0, 0, 0, 4, 0, 0, 0, 5];
        let fields = [
            5, 1, b'u', 0, 0, 0, 0, 9, // REPLY_SERIAL = 9
            8, 1, b'g', 0, 1, b'u', 0, // SIGNATURE = "u"
        ];
        // the array length leaves out the padding that ends the header
        m.extend_from_slice(&(fields.len() as u32).to_be_bytes());
        m.extend_from_slice(&fields);
        m.push(0);
        m.extend_from_slice(&[1, 2, 3, 4]);
        let msg = Message::decode(&mut m.as_slice()).unwrap();
        assert_eq!(msg.kind, Kind::MethodReturn);
        assert_eq!(msg.reply_serial, Some(9));
        assert_eq!(msg.body, [Value::U32(0x0102_0304)]);
    }

    #[test]
    fn addresses_are_parsed_and_unescaped() {
        let a = parse_address("unix:path=/run/user/1000/bus").unwrap();
        assert_eq!(
            a.as_pathname(),
            Some(std::path::Path::new("/run/user/1000/bus"))
        );
        let a = parse_address("tcp:host=x;unix:path=/tmp/a%20b,guid=00").unwrap();
        assert_eq!(a.as_pathname(), Some(std::path::Path::new("/tmp/a b")));
        use std::os::linux::net::SocketAddrExt;
        let a = parse_address("unix:abstract=/tmp/dbus-x").unwrap();
        assert_eq!(a.as_abstract_name(), Some(&b"/tmp/dbus-x"[..]));
        assert!(parse_address("tcp:host=localhost").is_err());
    }
}
