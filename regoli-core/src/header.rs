use crate::MAX_RANK;

/// The validated fields and data payload of one NPY file.
pub struct Header<'a> {
    /// Primitive dtype descriptor, such as `<f8`.
    pub dtype: String,
    /// Logical axis lengths in NumPy order.
    pub shape: Vec<usize>,
    /// Whether elements are stored in Fortran (column-major) order.
    pub fortran: bool,
    /// Bytes immediately following the NPY header.
    pub data: &'a [u8],
}

/// Parse the NPY preamble and its restricted, dictionary-shaped header.
///
/// Only standard primitive dtype descriptors and the three required header
/// fields are accepted; the header is parsed as data, never evaluated as
/// Python code.
pub fn parse(bytes: &[u8]) -> Result<Header<'_>, String> {
    if bytes.get(..6) != Some(b"\x93NUMPY") {
        return Err("invalid NPY magic signature".into());
    }
    let (prefix, length) = match bytes.get(6..8) {
        Some([1, 0]) => {
            let size: [u8; 2] = bytes
                .get(8..10)
                .ok_or("truncated NPY header length")?
                .try_into()
                .map_err(|_| "invalid NPY header length")?;
            (10, u16::from_le_bytes(size) as usize)
        }
        Some([2 | 3, 0]) => {
            let size: [u8; 4] = bytes
                .get(8..12)
                .ok_or("truncated NPY header length")?
                .try_into()
                .map_err(|_| "invalid NPY header length")?;
            (12, u32::from_le_bytes(size) as usize)
        }
        _ => return Err("unsupported or truncated NPY version (expected 1.0, 2.0, or 3.0)".into()),
    };
    if length > 65536 {
        return Err("NPY header exceeds 64 KiB".into());
    }
    // Header lengths are little-endian in every supported NPY version. Keep
    // the payload slice borrowed from the original input instead of copying it.
    let end = prefix + length;
    let raw = bytes.get(prefix..end).ok_or("truncated NPY header")?;
    if raw.last() != Some(&b'\n') {
        return Err("NPY header must end with a newline".into());
    }
    let mut parser = Parser { input: raw };
    parser.expect(b'{')?;
    let (mut dtype, mut shape, mut fortran) = (None, None, None);
    while !parser.consume(b'}') {
        let key = parser.string()?;
        parser.expect(b':')?;
        match key.as_str() {
            "descr" if dtype.is_none() => {
                if !matches!(parser.peek(), Some(b'\'' | b'"')) {
                    return Err("structured and subarray dtypes are not supported".into());
                }
                dtype = Some(parser.string()?);
            }
            "shape" if shape.is_none() => shape = Some(parser.shape()?),
            "fortran_order" if fortran.is_none() => {
                parser.whitespace();
                if let Some(rest) = parser.input.strip_prefix(b"True") {
                    fortran = Some(true);
                    parser.input = rest;
                } else if let Some(rest) = parser.input.strip_prefix(b"False") {
                    fortran = Some(false);
                    parser.input = rest;
                } else {
                    return Err("NPY fortran_order must be True or False".into());
                }
            }
            _ => return Err(format!("unknown or duplicate NPY header key: {key}")),
        }
        if !parser.consume(b',') {
            parser.expect(b'}')?;
            break;
        }
    }
    parser.whitespace();
    if !parser.input.is_empty() {
        return Err("unexpected text after NPY header dictionary".into());
    }
    Ok(Header {
        dtype: dtype.ok_or("NPY header is missing descr")?,
        shape: shape.ok_or("NPY header is missing shape")?,
        fortran: fortran.ok_or("NPY header is missing fortran_order")?,
        data: &bytes[end..],
    })
}

struct Parser<'a> {
    input: &'a [u8],
}

impl Parser<'_> {
    /// Advance past ASCII whitespace at the current input position.
    fn whitespace(&mut self) {
        while self.input.first().is_some_and(u8::is_ascii_whitespace) {
            self.input = &self.input[1..];
        }
    }

    /// Return the next non-whitespace byte without consuming it.
    fn peek(&mut self) -> Option<u8> {
        self.whitespace();
        self.input.first().copied()
    }

    /// Consume `byte` if it is the next non-whitespace byte.
    fn consume(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.input = &self.input[1..];
            true
        } else {
            false
        }
    }

    /// Require and consume `byte`, otherwise return a header syntax error.
    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.consume(byte) {
            Ok(())
        } else {
            Err(format!("expected '{}' in NPY header", byte as char))
        }
    }

    /// Read an unescaped ASCII string delimited by matching single or double quotes.
    fn string(&mut self) -> Result<String, String> {
        let quote = self.peek().ok_or("truncated NPY string")?;
        if !matches!(quote, b'\'' | b'"') {
            return Err("expected a quoted NPY header string".into());
        }
        self.input = &self.input[1..];
        let length = self
            .input
            .iter()
            .position(|&byte| byte == quote)
            .ok_or("unterminated NPY header string")?;
        let raw = &self.input[..length];
        if raw.iter().any(|&byte| !byte.is_ascii() || byte == b'\\') {
            return Err("only unescaped ASCII dtype and header keys are supported".into());
        }
        let result = String::from_utf8(raw.to_vec()).map_err(|e| e.to_string())?;
        self.input = &self.input[length + 1..];
        Ok(result)
    }

    /// Parse a tuple-like sequence of non-negative dimensions.
    ///
    /// A trailing comma is required for a one-dimensional shape, matching
    /// Python's syntax for a single-item tuple.
    fn shape(&mut self) -> Result<Vec<usize>, String> {
        self.expect(b'(')?;
        let mut shape = Vec::new();
        while !self.consume(b')') {
            if shape.len() == MAX_RANK {
                return Err("array rank exceeds 32".into());
            }
            self.whitespace();
            let length = self.input.iter().take_while(|b| b.is_ascii_digit()).count();
            if length == 0 {
                return Err("NPY dimensions must be nonnegative integers".into());
            }
            let number = std::str::from_utf8(&self.input[..length])
                .map_err(|e| e.to_string())?
                .parse::<usize>()
                .map_err(|_| "NPY dimension exceeds the platform integer range")?;
            shape.push(number);
            self.input = &self.input[length..];
            if !self.consume(b',') {
                if shape.len() == 1 {
                    return Err("one-dimensional NPY shape requires a trailing comma".into());
                }
                self.expect(b')')?;
                break;
            }
        }
        Ok(shape)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal NPY v1 file from a textual header for parser tests.
    fn npy(header: &str) -> Vec<u8> {
        let raw = format!("{header}\n");
        let mut bytes = b"\x93NUMPY\x01\x00".to_vec();
        bytes.extend_from_slice(&(raw.len() as u16).to_le_bytes());
        bytes.extend_from_slice(raw.as_bytes());
        bytes
    }

    #[test]
    /// Accept header fields in any order and either quote style.
    fn permits_header_key_order_and_quote_styles() {
        let data = npy(r#"{"shape": (2, 0), "fortran_order": True, "descr": ">f4"}"#);
        let header = parse(&data).unwrap();
        assert_eq!(header.shape, vec![2, 0]);
        assert_eq!(header.dtype, ">f4");
        assert!(header.fortran);
    }

    #[test]
    /// Reject missing, duplicated, malformed, oversized, or trailing header data.
    fn rejects_malformed_headers() {
        for header in [
            "{}",
            "{'descr':'<i4','descr':'<f4','shape':(),'fortran_order':False}",
            "{'descr':'<i4','shape':(-1,),'fortran_order':False}",
            "{'descr':'<i4','shape':(1),'fortran_order':False}",
            "{'descr':'<i4','shape':(),'fortran_order':0}",
            "{'descr':'<i4','shape':(),'fortran_order':False} garbage",
            "{'descr':'<i4','shape':(18446744073709551616,),'fortran_order':False}",
            "{'descr':'<i4','shape':((((((((((0,),),),),),),),),),),'fortran_order':False}",
        ] {
            assert!(parse(&npy(header)).is_err(), "{header}");
        }
        assert!(parse(b"\x93NUMPY\x02\0\xff\xff\xff\xff").is_err());
        assert!(parse(b"\x93NUMPY\x01\0\x01\0").is_err());
    }

    #[test]
    /// Accept a header exactly at the size limit and reject one byte over it.
    fn header_size_limit_is_exact() {
        for size in [65536u32, 65537] {
            let mut bytes = b"\x93NUMPY\x02\0".to_vec();
            bytes.extend_from_slice(&size.to_le_bytes());
            let header = b"{'descr':'<i4','shape':(),'fortran_order':False}";
            bytes.extend_from_slice(header);
            bytes.resize(12 + size as usize - 1, b' ');
            bytes.push(b'\n');
            assert_eq!(parse(&bytes).is_ok(), size == 65536);
        }
    }
}
