use ciborium::value::Value;
use std::io::{Cursor, Read};

mod header;

#[cfg(target_arch = "wasm32")]
wasm_minimal_protocol::initiate_protocol!();

const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_NODES: usize = 1_000_000;
const MAX_RANK: usize = 32;
const MAX_ENTRIES: usize = 1024;

#[derive(Clone, Copy, Debug)]
struct RowSelection {
    every: usize,
    start: usize,
    max_rows: Option<usize>,
}

impl RowSelection {
    fn all() -> Self {
        Self {
            every: 1,
            start: 0,
            max_rows: None,
        }
    }

    fn parse(bytes: &[u8]) -> Result<Self, String> {
        let value: Value = ciborium::from_reader(bytes).map_err(error)?;
        let Value::Map(fields) = value else {
            return Err(error("decoder options must be a CBOR map"));
        };
        let get = |name: &str| {
            fields
                .iter()
                .find(|(key, _)| key == &Value::Text(name.into()))
                .map(|(_, value)| value)
        };
        let integer = |name: &str, default: usize| -> Result<usize, String> {
            let Some(value) = get(name) else {
                return Ok(default);
            };
            let Value::Integer(value) = value else {
                return Err(error(format!("{name} must be a non-negative integer")));
            };
            usize::try_from(i128::from(*value))
                .map_err(|_| error(format!("{name} must be a non-negative integer")))
        };
        let every = integer("every", 1)?;
        if every == 0 {
            return Err(error("every must be a positive integer"));
        }
        let start = integer("start", 0)?;
        let max_rows = match get("max-rows") {
            None | Some(Value::Null) => None,
            Some(Value::Integer(value)) => Some(
                usize::try_from(i128::from(*value))
                    .map_err(|_| error("max-rows must be a non-negative integer or none"))?,
            ),
            Some(_) => {
                return Err(error("max-rows must be a non-negative integer or none"));
            }
        };
        Ok(Self {
            every,
            start,
            max_rows,
        })
    }

    fn rows(self, count: usize) -> impl Iterator<Item = usize> {
        (self.start..count)
            .step_by(self.every)
            .take(self.max_rows.unwrap_or(usize::MAX))
    }
}

fn validate_scalar(bytes: &[u8], kind: u8, big_endian: bool) -> Result<(), String> {
    if kind == b'u' {
        i64::try_from(unsigned(bytes, big_endian))
            .map_err(|_| "uint64 value exceeds Typst's exact integer range (2^63 - 1)")?;
    }
    Ok(())
}

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
pub fn decode(input: &[u8]) -> Result<Vec<u8>, String> {
    decode_with_selection(input, RowSelection::all())
}

/// Decode an archive, applying row selection before constructing output values.
#[cfg_attr(target_arch = "wasm32", wasm_minimal_protocol::wasm_func)]
pub fn decode_with_options(input: &[u8], options: &[u8]) -> Result<Vec<u8>, String> {
    let selection = RowSelection::parse(options)?;
    decode_with_selection(input, selection)
}

fn decode_with_selection(input: &[u8], selection: RowSelection) -> Result<Vec<u8>, String> {
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
        let mut entry = archive
            .by_index_with_options(
                index,
                zip::ZipReadOptions::new().enforce_header_consistency(true),
            )
            .map_err(error)?;
        if entry.is_dir() {
            continue;
        }
        let archive_name = entry.name().map_err(error)?.into_owned();
        if entry.enclosed_name().is_none() {
            return Err(error(format!("invalid archive entry path: {archive_name}")));
        }
        let name = archive_name
            .strip_suffix(".npy")
            .ok_or_else(|| error(format!("non-NPY archive entry: {archive_name}")))?
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
        let array = decode_npy(&bytes, &mut budget, selection)
            .map_err(|e| error(format!("{name}: {e}")))?;
        output.push((Value::Text(name), array));
    }
    let mut encoded = Vec::new();
    ciborium::into_writer(&Value::Map(output), &mut encoded).map_err(error)?;
    Ok(encoded)
}

fn decode_npy(bytes: &[u8], budget: &mut usize, selection: RowSelection) -> Result<Value, String> {
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
    // Validate every element, including rows omitted from the output.
    for chunk in data.chunks_exact(width) {
        validate_scalar(chunk, kind, big_endian)?;
    }
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
    let selected_shape = if shape.is_empty() {
        shape.clone()
    } else {
        let selected_rows = selection.rows(shape[0]).count();
        let mut selected_shape = shape.clone();
        selected_shape[0] = selected_rows;
        selected_shape
    };
    let nested = if shape.is_empty() {
        scalar(data, kind, big_endian)?
    } else {
        Value::Array(
            selection
                .rows(shape[0])
                .map(|row| {
                    nest_npy(
                        data,
                        kind,
                        width,
                        big_endian,
                        &shape[1..],
                        &strides[1..],
                        row * strides[0],
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
        )
    };
    Ok(map([
        ("data", nested),
        (
            "shape",
            Value::Array(
                selected_shape
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

fn nest_npy(
    data: &[u8],
    kind: u8,
    width: usize,
    big_endian: bool,
    shape: &[usize],
    strides: &[usize],
    offset: usize,
) -> Result<Value, String> {
    if shape.is_empty() {
        let start = offset * width;
        scalar(&data[start..start + width], kind, big_endian)
    } else {
        Ok(Value::Array(
            (0..shape[0])
                .map(|i| {
                    nest_npy(
                        data,
                        kind,
                        width,
                        big_endian,
                        &shape[1..],
                        &strides[1..],
                        offset + i * strides[0],
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture(name: &str) -> Vec<u8> {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("regoli-core must be inside the repository");
        std::fs::read(repo_root.join("tests").join("fixtures").join(name))
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
            ("duplicate.npz", "duplicate"),
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
        assert!(decode_npy(&entry, &mut exact, RowSelection::all()).is_ok());
        assert_eq!(exact, 0);
        assert!(
            decode_npy(&entry, &mut 2, RowSelection::all())
                .unwrap_err()
                .contains("array-node limit")
        );
    }

    #[test]
    fn row_selection_is_applied_before_values_are_built() {
        let encoded = decode_with_selection(
            &fixture("compressed.npz"),
            RowSelection {
                every: 2,
                start: 1,
                max_rows: Some(1),
            },
        )
        .unwrap();
        let value: Value = ciborium::from_reader(encoded.as_slice()).unwrap();
        let arrays = value.as_map().unwrap();
        let matrix = arrays
            .iter()
            .find(|(name, _)| name == &Value::Text("matrix".into()))
            .unwrap()
            .1
            .as_map()
            .unwrap();
        let data = &matrix
            .iter()
            .find(|(name, _)| name == &Value::Text("data".into()))
            .unwrap()
            .1;
        assert_eq!(
            data,
            &Value::Array(vec![Value::Array(vec![
                Value::Float(3.0),
                Value::Float(4.0),
                Value::Float(5.0),
            ])])
        );
        let shape = &matrix
            .iter()
            .find(|(name, _)| name == &Value::Text("shape".into()))
            .unwrap()
            .1;
        assert_eq!(
            shape,
            &Value::Array(vec![Value::Integer(1.into()), Value::Integer(3.into())])
        );
    }

    #[test]
    fn row_selection_still_validates_omitted_values() {
        let result = decode_with_selection(
            &fixture("uint64-overflow.npz"),
            RowSelection {
                every: 2,
                start: 1,
                max_rows: Some(0),
            },
        )
        .unwrap_err();
        assert!(result.contains("exact integer range"));
    }

    #[test]
    fn row_selection_options_are_validated() {
        for (options, message) in [
            (
                Value::Map(vec![(
                    Value::Text("every".into()),
                    Value::Integer(0.into()),
                )]),
                "every must be a positive integer",
            ),
            (
                Value::Map(vec![(
                    Value::Text("start".into()),
                    Value::Integer((-1).into()),
                )]),
                "start must be a non-negative integer",
            ),
            (
                Value::Map(vec![(
                    Value::Text("max-rows".into()),
                    Value::Text("many".into()),
                )]),
                "max-rows must be a non-negative integer or none",
            ),
        ] {
            let mut encoded = Vec::new();
            ciborium::into_writer(&options, &mut encoded).unwrap();
            assert!(RowSelection::parse(&encoded).unwrap_err().contains(message));
        }
    }
}
