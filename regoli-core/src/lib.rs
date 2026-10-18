use ciborium::value::Value;
use std::io::{Cursor, Read};

mod header;

#[cfg(target_arch = "wasm32")]
wasm_minimal_protocol::initiate_protocol!();

const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_NODES: usize = 1_000_000;
const MAX_RANK: usize = 32;
const MAX_ENTRIES: usize = 1024;

fn error(message: impl std::fmt::Display) -> String {
    format!("regoli: {message}")
}

fn map(fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Map(
        fields
            .into_iter()
            .map(|(key, value)| (Value::Text(key.into()), value))
            .collect(),
    )
}

/// Decode without filesystem access or state shared between plugin calls.
#[cfg_attr(target_arch = "wasm32", wasm_minimal_protocol::wasm_func)]
pub fn decode(input: &[u8]) -> Result<Vec<u8>, String> {
    if input.len() > MAX_BYTES {
        return Err(error("archive exceeds the 64 MiB input limit"));
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(input)).map_err(error)?;
    if archive.len() > MAX_ENTRIES {
        return Err(error("archive exceeds the 1024-entry limit"));
    }
    let mut output = Vec::new();
    let mut total_bytes = 0usize;
    let mut budget = MAX_NODES;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(error)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry
            .name()
            .strip_suffix(".npy")
            .ok_or_else(|| error(format!("non-NPY archive entry: {}", entry.name())))?
            .to_owned();
        if output
            .iter()
            .any(|(key, _)| key == &Value::Text(name.clone()))
        {
            return Err(error(format!("duplicate array name: {name}")));
        }
        let size = usize::try_from(entry.size()).map_err(error)?;
        total_bytes = total_bytes
            .checked_add(size)
            .filter(|&n| n <= MAX_BYTES)
            .ok_or_else(|| error("archive exceeds the 64 MiB decompressed limit"))?;
        let mut bytes = Vec::new();
        // Read to EOF to verify the ZIP CRC, but never trust the advertised size.
        entry
            .by_ref()
            .take((MAX_BYTES - (total_bytes - size) + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| error(format!("{name}: {e}")))?;
        if bytes.len() != size {
            return Err(error(format!("{name}: invalid decompressed entry size")));
        }
        let array = decode_npy(&bytes, &mut budget).map_err(|e| error(format!("{name}: {e}")))?;
        output.push((Value::Text(name), array));
    }
    let mut encoded = Vec::new();
    ciborium::into_writer(&Value::Map(output), &mut encoded).map_err(error)?;
    Ok(encoded)
}

fn decode_npy(bytes: &[u8], budget: &mut usize) -> Result<Value, String> {
    let header::Header {
        dtype,
        shape,
        fortran,
        data,
    } = header::parse(bytes)?;
    let mut count = 1usize;
    let mut nodes = 1usize;
    // Count container nodes as well as scalars, including shapes such as (N, 0).
    for &dimension in &shape {
        count = count
            .checked_mul(dimension)
            .ok_or("array shape overflows")?;
        nodes = nodes.checked_add(count).ok_or("array shape overflows")?;
    }
    *budget = budget
        .checked_sub(nodes)
        .ok_or("archive exceeds the 1,000,000 array-node limit")?;
    let (kind, width, big_endian) = parse_dtype(&dtype)?;
    if count.checked_mul(width) != Some(data.len()) {
        return Err("NPY data length does not match shape and dtype".into());
    }
    let values = data
        .chunks_exact(width)
        .map(|chunk| scalar(chunk, kind, big_endian))
        .collect::<Result<Vec<_>, _>>()?;
    let mut strides = vec![1usize; shape.len()];
    if fortran {
        for i in 1..shape.len() {
            strides[i] = strides[i - 1]
                .checked_mul(shape[i - 1])
                .ok_or("array strides overflow")?;
        }
    } else {
        for i in (0..shape.len().saturating_sub(1)).rev() {
            strides[i] = strides[i + 1]
                .checked_mul(shape[i + 1])
                .ok_or("array strides overflow")?;
        }
    }
    let nested = nest(&values, &shape, &strides, 0);
    Ok(map([
        ("data", nested),
        (
            "shape",
            Value::Array(
                shape
                    .iter()
                    .map(|&n| Value::Integer((n as u64).into()))
                    .collect(),
            ),
        ),
        ("dtype", Value::Text(dtype)),
        ("fortran-order", Value::Bool(fortran)),
    ]))
}

fn parse_dtype(dtype: &str) -> Result<(u8, usize, bool), String> {
    let bytes = dtype.as_bytes();
    if bytes.get(1) == Some(&b'O') {
        return Err("unsupported dtype: object arrays (pickle) are never loaded".into());
    }
    if bytes.len() < 3 {
        return Err(format!("invalid dtype: {dtype}"));
    }
    if !matches!(bytes[0], b'<' | b'>' | b'|') {
        return Err(format!("invalid or nonportable dtype byte order: {dtype}"));
    }
    let width: usize = dtype[2..]
        .parse()
        .map_err(|_| format!("invalid dtype: {dtype}"))?;
    let kind = bytes[1];
    let supported = match kind {
        b'b' => width == 1,
        b'i' | b'u' => matches!(width, 1 | 2 | 4 | 8),
        b'f' => matches!(width, 2 | 4 | 8),
        b'c' => matches!(width, 8 | 16),
        _ => false,
    };
    if !supported || (bytes[0] == b'|' && width != 1) {
        return Err(format!(
            "unsupported dtype {dtype}; expected boolean, integer, float16/32/64, or complex64/128"
        ));
    }
    Ok((kind, width, bytes[0] == b'>'))
}

fn unsigned(bytes: &[u8], big_endian: bool) -> u64 {
    let mut padded = [0u8; 8];
    if big_endian {
        padded[8 - bytes.len()..].copy_from_slice(bytes);
        u64::from_be_bytes(padded)
    } else {
        padded[..bytes.len()].copy_from_slice(bytes);
        u64::from_le_bytes(padded)
    }
}

fn floating(bytes: &[u8], big_endian: bool) -> f64 {
    let bits = unsigned(bytes, big_endian);
    match bytes.len() {
        2 => half::f16::from_bits(bits as u16).to_f64(),
        4 => f32::from_bits(bits as u32) as f64,
        8 => f64::from_bits(bits),
        _ => unreachable!(),
    }
}

fn scalar(bytes: &[u8], kind: u8, big_endian: bool) -> Result<Value, String> {
    Ok(match kind {
        b'b' => Value::Bool(bytes[0] != 0),
        b'u' => {
            let value = unsigned(bytes, big_endian);
            let value = i64::try_from(value)
                .map_err(|_| "uint64 value exceeds Typst's exact integer range (2^63 - 1)")?;
            Value::Integer(value.into())
        }
        b'i' => {
            let shift = 64 - bytes.len() * 8;
            let value = ((unsigned(bytes, big_endian) << shift) as i64) >> shift;
            Value::Integer(value.into())
        }
        b'f' => Value::Float(floating(bytes, big_endian)),
        b'c' => {
            let (real, imaginary) = bytes.split_at(bytes.len() / 2);
            map([
                ("real", Value::Float(floating(real, big_endian))),
                ("imag", Value::Float(floating(imaginary, big_endian))),
            ])
        }
        _ => unreachable!(),
    })
}

fn nest(values: &[Value], shape: &[usize], strides: &[usize], offset: usize) -> Value {
    if shape.is_empty() {
        values[offset].clone()
    } else {
        Value::Array(
            (0..shape[0])
                .map(|i| nest(values, &shape[1..], &strides[1..], offset + i * strides[0]))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(Path::new("tests").join("fixtures").join(name))
            .expect("run python scripts/generate_fixtures.py first")
    }

    #[test]
    fn compressed_and_stored_match() {
        let stored = decode(&fixture("stored.npz")).unwrap();
        let compressed = decode(&fixture("compressed.npz")).unwrap();
        assert_eq!(stored, compressed);
    }

    #[test]
    fn rejects_invalid_archives() {
        for (name, message) in [
            ("object.npz", "unsupported dtype"),
            ("strings.npz", "unsupported dtype"),
            ("structured.npz", "structured"),
            ("uint64-overflow.npz", "exact integer range"),
            ("truncated.npz", "data length"),
            ("trailing.npz", "data length"),
            ("duplicate.npz", "duplicate array name"),
            ("non-npy.npz", "non-NPY"),
            ("bad-crc.npz", "checksum"),
            ("too-many-nodes.npz", "array-node limit"),
            ("too-many-entries.npz", "1024-entry limit"),
            ("too-many-axes.npz", "rank exceeds"),
            ("bad-npy.npz", "magic"),
        ] {
            let result = decode(&fixture(name)).unwrap_err();
            assert!(
                result.to_lowercase().contains(&message.to_lowercase()),
                "{name}: expected {message}, got {result}"
            );
        }
        assert!(decode(b"not a zip").is_err());
        assert!(decode(&vec![0; MAX_BYTES + 1]).is_err());
    }

    #[test]
    fn empty_archive_is_a_dictionary() {
        let encoded = decode(&fixture("empty.npz")).unwrap();
        let value: Value = ciborium::from_reader(encoded.as_slice()).unwrap();
        assert_eq!(value, Value::Map(vec![]));
    }

    #[test]
    fn rejects_unsupported_scalar_formats() {
        for dtype in [
            "<f16", "<c32", "|S4", "|V8", "<M8", "<u3", "<b2", "=i4", "|f8",
        ] {
            assert!(parse_dtype(dtype).is_err(), "{dtype}");
        }
    }

    #[test]
    fn node_budget_boundary_is_exact() {
        let data = fixture("positional.npz");
        let mut archive = zip::ZipArchive::new(Cursor::new(data)).unwrap();
        let mut entry = Vec::new();
        archive
            .by_name("arr_0.npy")
            .unwrap()
            .read_to_end(&mut entry)
            .unwrap();
        let mut exact = 3;
        assert!(decode_npy(&entry, &mut exact).is_ok());
        assert_eq!(exact, 0);
        assert!(decode_npy(&entry, &mut 2)
            .unwrap_err()
            .contains("array-node limit"));
    }
}
