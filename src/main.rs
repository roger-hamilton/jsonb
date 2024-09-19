use std::{
    io::{Read, Write},
    ops::Index,
};

enum JValue<'a> {
    Object(Vec<(&'a str, JValue<'a>)>),
    Array(Vec<JValue<'a>>),
    String(&'a str),
    Number(f64),
    True,
    False,
    Null,
}

trait BitWriter {
    // Writes a single bit to the stream.
    fn write_bit(&mut self, bit: bool);

    /// Writes `count` bits from `bits` to the stream.
    /// starts from the least significant bit.
    fn write_bits(&mut self, bits: u64, count: usize) {
        for i in 0..count {
            self.write_bit((bits >> i) & 1 == 1);
        }
    }

    fn write_f64(&mut self, value: f64) {
        self.write_bits(value.to_bits(), 64);
    }
}
#[derive(Default)]
struct BitVec {
    data: Vec<u8>,
    bit_offset: u8,
}

impl BitVec {
    fn new() -> Self {
        BitVec::default()
    }

    fn with_capacity(capacity: usize) -> Self {
        BitVec {
            data: Vec::with_capacity(capacity),
            bit_offset: 0,
        }
    }

    fn get(&self, index: usize) -> Option<bool> {
        let byte = self.data.get(index / 8)?;
        let bit = index % 8;
        Some((byte >> bit) & 1 == 1)
    }
}

impl BitWriter for BitVec {
    fn write_bit(&mut self, bit: bool) {
        if self.bit_offset == 0 {
            self.data.push(0);
        }

        if bit {
            let last = self.data.last_mut().unwrap();
            *last |= 1 << self.bit_offset;
        }

        self.bit_offset = (self.bit_offset + 1) % 8;
    }
}

fn write_length<W: Write>(writer: &mut W, length: u64) -> std::io::Result<usize> {
    let mut length = length;
    let mut bytes_written = 0;
    while length >= 0b1000_0000 {
        bytes_written += writer.write(&[0b1000_0000 | (length as u8)])?;
        length >>= 7;
    }
    bytes_written += writer.write(&[length as u8])?;
    Ok(bytes_written)
}

fn write_str<W: Write>(writer: &mut W, s: &str) -> std::io::Result<usize> {
    let mut bytes_written = 0;
    bytes_written += write_length(writer, s.len() as u64)?;
    bytes_written += writer.write(s.as_bytes())?;
    Ok(bytes_written)
}

const NULL_MARKER: u8 = 0b0000_0000;
const TRUE_MARKER: u8 = 0b0000_0001;
const FALSE_MARKER: u8 = 0b0000_0010;
const NUMBER_MARKER: u8 = 0b0000_0011;
const STRING_MARKER: u8 = 0b0000_0100;
const ARRAY_MARKER: u8 = 0b0000_0101;
const OBJECT_MARKER: u8 = 0b0000_0110;

fn serialize_to<W: Write>(writer: &mut W, value: &JValue) -> std::io::Result<usize> {
    match value {
        // 0000
        JValue::Null => writer.write(&[NULL_MARKER]),
        // 0001
        JValue::True => writer.write(&[TRUE_MARKER]),
        // 0010
        JValue::False => writer.write(&[FALSE_MARKER]),
        JValue::Number(n) => {
            let mut bytes_written = 0;
            bytes_written += writer.write(&[NUMBER_MARKER])?;
            bytes_written += writer.write(&n.to_be_bytes())?;
            Ok(bytes_written)
        }
        JValue::String(s) => {
            let mut bytes_written = 0;
            // + 1 byte
            bytes_written += writer.write(&[STRING_MARKER])?;
            // + 8 bytes
            bytes_written += write_str(writer, s)?;
            Ok(bytes_written)
        }
        JValue::Array(values) => {
            let mut bytes_written = 0;
            bytes_written += writer.write(&[ARRAY_MARKER])?;
            bytes_written += write_length(writer, values.len() as u64)?;
            for value in values {
                bytes_written += serialize_to(writer, value)?;
            }
            Ok(bytes_written)
        }
        JValue::Object(values) => {
            let mut bytes_written = 0;
            bytes_written += writer.write(&[OBJECT_MARKER])?;
            bytes_written += write_length(writer, values.len() as u64)?;
            for (key, value) in values {
                bytes_written += write_str(writer, key)?;
                bytes_written += serialize_to(writer, value)?;
            }
            Ok(bytes_written)
        }
    }
}

fn read_length<R: Read>(reader: &mut R) -> std::io::Result<u64> {
    let mut length = 0u64;
    let mut shift = 0;
    loop {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        length |= ((buf[0] & 0b0111_1111) as u64) << shift;
        shift += 7;
        if buf[0] & 0b1000_0000 == 0 {
            break;
        }
    }
    Ok(length)
}

fn read_from<'a, 'b, R: Read>(reader: &'a mut R) -> std::io::Result<JValue<'b>> {
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf)?;
    match buf[0] {
        NULL_MARKER => Ok(JValue::Null),
        TRUE_MARKER => Ok(JValue::True),
        FALSE_MARKER => Ok(JValue::False),
        NUMBER_MARKER => {
            let mut buf = [0u8; 8];
            reader.read_exact(&mut buf)?;
            Ok(JValue::Number(f64::from_be_bytes(buf)))
        }
        STRING_MARKER => {
            let length = read_length(reader)?;
            let mut buf = vec![0u8; length as usize];
            reader.read_exact(&mut buf)?;
            Ok(JValue::String(std::str::from_utf8(&buf).unwrap()))
        }
        ARRAY_MARKER => {
            let length = read_length(reader)?;
            let mut values = Vec::with_capacity(length as usize);
            for _ in 0..length {
                values.push(read_from(reader)?);
            }
            Ok(JValue::Array(values))
        }
        OBJECT_MARKER => {
            let length = read_length(reader)?;
            let mut values = Vec::with_capacity(length as usize);
            for _ in 0..length {
                let key_length = read_length(reader)?;
                let mut key_buf = vec![0u8; key_length as usize];
                reader.read_exact(&mut key_buf)?;
                let key = std::str::from_utf8(&key_buf).unwrap();
                let value = read_from(reader)?;
                values.push((key, value));
            }
            Ok(JValue::Object(values))
        }
        _ => panic!("Invalid marker"),
    }
}
fn main() {
    println!("Hello, world!");
}
