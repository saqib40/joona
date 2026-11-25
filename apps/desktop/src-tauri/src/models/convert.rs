//! Converts OpenAI's PyTorch Whisper checkpoints into whisper.cpp's GGML
//! format, so Whisper can be installed from OpenAI's own servers when Hugging
//! Face is unreachable. The output is byte-for-byte what whisper.cpp's
//! `convert-pt-to-ggml.py` writes, without needing Python or PyTorch.

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, Write};
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD_NO_PAD as BASE64_NO_PAD;
use half::{bf16, f16};

use super::pickle::{self, DType, TensorRef, Value};

const GGML_MAGIC: u32 = 0x6767_6d6c;
/// These stay in f32 even in an f16 model, as whisper.cpp expects.
const F32_TENSORS: &[&str] = &[
    "encoder.conv1.bias",
    "encoder.conv2.bias",
    "encoder.positional_embedding",
    "decoder.positional_embedding",
];

/// `assets` holds `mel_filters.npz`, `multilingual.tiktoken` and `gpt2.tiktoken`
/// from the openai/whisper repository.
pub fn pt_to_ggml(
    pt: &Path,
    assets: &Path,
    out: &Path,
    mut progress: impl FnMut(f32),
) -> anyhow::Result<()> {
    let mut zip = zip::ZipArchive::new(BufReader::new(File::open(pt)?))?;
    let pkl_name: String = zip
        .file_names()
        .filter_map(Result::ok)
        .find(|n| n.ends_with("/data.pkl") || n == "data.pkl")
        .map(|n| n.into_owned())
        .ok_or_else(|| anyhow::anyhow!("not a PyTorch checkpoint: data.pkl is missing"))?;
    let prefix = pkl_name.trim_end_matches("data.pkl").to_owned();
    let mut pkl = Vec::new();
    zip.by_name(&pkl_name)?.read_to_end(&mut pkl)?;
    let root = pickle::parse(&pkl)?;

    let dims = root
        .get("dims")
        .ok_or_else(|| anyhow::anyhow!("checkpoint has no model dimensions"))?;
    let dim = |k: &str| -> anyhow::Result<i32> {
        dims.get(k)
            .and_then(Value::as_int)
            .map(|v| v as i32)
            .ok_or_else(|| anyhow::anyhow!("checkpoint is missing {k}"))
    };
    let Some(Value::Dict(state)) = root.get("model_state_dict") else {
        anyhow::bail!("checkpoint has no weights");
    };

    let n_mels = dim("n_mels")?;
    let n_vocab = dim("n_vocab")?;
    let multilingual = n_vocab >= 51865;
    let (rows, cols, filters) = read_npz_f32(
        &assets.join("mel_filters.npz"),
        &format!("mel_{n_mels}.npy"),
    )?;
    let tokens = read_tiktoken(&assets.join(if multilingual {
        "multilingual.tiktoken"
    } else {
        "gpt2.tiktoken"
    }))?;

    let part = out.with_extension("part");
    let mut w = BufWriter::with_capacity(1 << 20, File::create(&part)?);
    let i32le = |w: &mut BufWriter<File>, v: i32| w.write_all(&v.to_le_bytes());
    i32le(&mut w, GGML_MAGIC as i32)?;
    for k in [
        "n_vocab",
        "n_audio_ctx",
        "n_audio_state",
        "n_audio_head",
        "n_audio_layer",
        "n_text_ctx",
        "n_text_state",
        "n_text_head",
        "n_text_layer",
        "n_mels",
    ] {
        i32le(&mut w, dim(k)?)?;
    }
    i32le(&mut w, 1)?; // f16 weights

    i32le(&mut w, rows as i32)?;
    i32le(&mut w, cols as i32)?;
    for &v in &filters {
        // The published GGML files were made from an older mel_filters.npz
        // that had +0.0 wherever the current one has -0.0.
        let v = if v == 0.0 { 0.0f32 } else { v };
        w.write_all(&v.to_le_bytes())?;
    }

    i32le(&mut w, tokens.len() as i32)?;
    for t in &tokens {
        i32le(&mut w, t.len() as i32)?;
        w.write_all(t)?;
    }

    let total = state.len().max(1);
    for (i, (name, tensor)) in state.iter().enumerate() {
        let (Value::Str(name), Value::Tensor(t)) = (name, tensor) else {
            continue;
        };
        write_tensor(&mut w, &mut zip, &prefix, name, t)?;
        progress((i + 1) as f32 / total as f32);
    }
    w.flush()?;
    drop(w);
    std::fs::rename(&part, out)?;
    Ok(())
}

fn write_tensor<R: Read + Seek>(
    w: &mut impl Write,
    zip: &mut zip::ZipArchive<R>,
    prefix: &str,
    name: &str,
    t: &TensorRef,
) -> anyhow::Result<()> {
    let mut shape: Vec<usize> = t.shape.iter().copied().filter(|&d| d != 1).collect();
    if name == "encoder.conv1.bias" || name == "encoder.conv2.bias" {
        shape = vec![shape.iter().product(), 1];
    }
    let n_dims = shape.len();
    let as_f32 = n_dims < 2 || F32_TENSORS.contains(&name);

    let values = read_elements(zip, prefix, t)?;

    w.write_all(&(n_dims as i32).to_le_bytes())?;
    w.write_all(&(name.len() as i32).to_le_bytes())?;
    w.write_all(&(if as_f32 { 0i32 } else { 1i32 }).to_le_bytes())?;
    for d in shape.iter().rev() {
        w.write_all(&(*d as i32).to_le_bytes())?;
    }
    w.write_all(name.as_bytes())?;

    let mut buf = Vec::with_capacity(values.len() * if as_f32 { 4 } else { 2 });
    match (&values, as_f32) {
        (Elements::F16(raw), false) => buf.extend_from_slice(raw),
        (Elements::F32(raw), true) => buf.extend_from_slice(raw),
        _ => {
            for i in 0..values.len() {
                let v = values.get(i);
                if as_f32 {
                    buf.extend_from_slice(&v.to_le_bytes());
                } else {
                    buf.extend_from_slice(&f16::from_f32(v).to_le_bytes());
                }
            }
        }
    }
    w.write_all(&buf)?;
    Ok(())
}

/// Tensor elements in row-major order, as raw little-endian bytes.
enum Elements {
    F16(Vec<u8>),
    BF16(Vec<u8>),
    F32(Vec<u8>),
    F64(Vec<u8>),
}

impl Elements {
    fn len(&self) -> usize {
        match self {
            Elements::F16(b) | Elements::BF16(b) => b.len() / 2,
            Elements::F32(b) => b.len() / 4,
            Elements::F64(b) => b.len() / 8,
        }
    }

    fn get(&self, i: usize) -> f32 {
        match self {
            Elements::F16(b) => f16::from_le_bytes([b[2 * i], b[2 * i + 1]]).to_f32(),
            Elements::BF16(b) => bf16::from_le_bytes([b[2 * i], b[2 * i + 1]]).to_f32(),
            Elements::F32(b) => f32::from_le_bytes(b[4 * i..4 * i + 4].try_into().unwrap()),
            Elements::F64(b) => f64::from_le_bytes(b[8 * i..8 * i + 8].try_into().unwrap()) as f32,
        }
    }
}

fn read_elements<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    prefix: &str,
    t: &TensorRef,
) -> anyhow::Result<Elements> {
    let mut storage = Vec::new();
    zip.by_name(&format!("{prefix}data/{}", t.storage))
        .map_err(|e| anyhow::anyhow!("tensor data {} is missing: {e}", t.storage))?
        .read_to_end(&mut storage)?;
    let size = t.dtype.size();
    let count: usize = t.shape.iter().product();

    let mut contiguous = vec![0usize; t.shape.len()];
    let mut acc = 1;
    for i in (0..t.shape.len()).rev() {
        contiguous[i] = acc;
        acc *= t.shape[i];
    }
    let bytes = if t.stride == contiguous || count <= 1 {
        let start = t.offset * size;
        let end = start + count * size;
        anyhow::ensure!(
            end <= storage.len(),
            "tensor data is shorter than its shape"
        );
        storage[start..end].to_vec()
    } else {
        let mut out = Vec::with_capacity(count * size);
        let mut index = vec![0usize; t.shape.len()];
        for _ in 0..count {
            let el = t.offset
                + index
                    .iter()
                    .zip(&t.stride)
                    .map(|(i, s)| i * s)
                    .sum::<usize>();
            anyhow::ensure!(
                (el + 1) * size <= storage.len(),
                "tensor stride points outside its data"
            );
            out.extend_from_slice(&storage[el * size..(el + 1) * size]);
            for d in (0..index.len()).rev() {
                index[d] += 1;
                if index[d] < t.shape[d] {
                    break;
                }
                index[d] = 0;
            }
        }
        out
    };
    Ok(match t.dtype {
        DType::F16 => Elements::F16(bytes),
        DType::BF16 => Elements::BF16(bytes),
        DType::F32 => Elements::F32(bytes),
        DType::F64 => Elements::F64(bytes),
    })
}

/// Reads a little-endian float32 array from a NumPy `.npz` archive.
fn read_npz_f32(path: &Path, entry: &str) -> anyhow::Result<(usize, usize, Vec<f32>)> {
    let mut zip = zip::ZipArchive::new(BufReader::new(File::open(path)?))?;
    let mut data = Vec::new();
    zip.by_name(entry)
        .map_err(|e| anyhow::anyhow!("{entry} not found in {}: {e}", path.display()))?
        .read_to_end(&mut data)?;
    parse_npy_f32(&data)
}

fn parse_npy_f32(data: &[u8]) -> anyhow::Result<(usize, usize, Vec<f32>)> {
    anyhow::ensure!(
        data.len() >= 10 && &data[..6] == b"\x93NUMPY",
        "not a NumPy array"
    );
    let (header_len, start): (usize, usize) = match data[6] {
        1 => (u16::from_le_bytes([data[8], data[9]]) as usize, 10),
        _ => {
            anyhow::ensure!(data.len() >= 12, "truncated NumPy header");
            (u32::from_le_bytes(data[8..12].try_into()?) as usize, 12)
        }
    };
    let end = start
        .checked_add(header_len)
        .filter(|&e| e <= data.len())
        .ok_or_else(|| anyhow::anyhow!("truncated NumPy header"))?;
    let header = std::str::from_utf8(&data[start..end])?;
    anyhow::ensure!(
        header.contains("'<f4'"),
        "expected little-endian float32, got {header}"
    );
    anyhow::ensure!(
        header.contains("'fortran_order': False"),
        "Fortran-ordered arrays aren't supported"
    );
    let shape_start = header
        .find("'shape': (")
        .ok_or_else(|| anyhow::anyhow!("array has no shape"))?
        + 10;
    let shape_end = shape_start
        + header[shape_start..]
            .find(')')
            .ok_or_else(|| anyhow::anyhow!("bad array shape"))?;
    let dims: Vec<usize> = header[shape_start..shape_end]
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    anyhow::ensure!(dims.len() == 2, "expected a 2-D array, got {dims:?}");
    let body = &data[end..];
    let count = dims[0]
        .checked_mul(dims[1])
        .ok_or_else(|| anyhow::anyhow!("array dimensions overflow"))?;
    let byte_len = count
        .checked_mul(4)
        .ok_or_else(|| anyhow::anyhow!("array byte length overflows"))?;
    anyhow::ensure!(body.len() >= byte_len, "array data is truncated");
    let values = body[..byte_len]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|&c| f32::from_le_bytes(c))
        .collect();
    Ok((dims[0], dims[1], values))
}

/// Token byte strings in rank order, from a `.tiktoken` file.
fn read_tiktoken(path: &Path) -> anyhow::Result<Vec<Vec<u8>>> {
    let text = std::fs::read_to_string(path)?;
    let mut tokens = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        // The end-of-text token is written as a bare "=", which Python reads as empty.
        let b64 = line
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .trim_end_matches('=');
        tokens.push(BASE64_NO_PAD.decode(b64)?);
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_npy_headers() {
        let mut data = b"\x93NUMPY\x01\x00".to_vec();
        let header = "{'descr': '<f4', 'fortran_order': False, 'shape': (2, 3), }";
        let padded = format!("{header:<54}\n");
        data.extend_from_slice(&(padded.len() as u16).to_le_bytes());
        data.extend_from_slice(padded.as_bytes());
        for v in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        let (r, c, v) = parse_npy_f32(&data).unwrap();
        assert_eq!((r, c), (2, 3));
        assert_eq!(v, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn rejects_other_dtypes() {
        let mut data = b"\x93NUMPY\x01\x00".to_vec();
        let header = "{'descr': '<f8', 'fortran_order': False, 'shape': (1, 1), }\n";
        data.extend_from_slice(&(header.len() as u16).to_le_bytes());
        data.extend_from_slice(header.as_bytes());
        data.extend_from_slice(&[0; 8]);
        assert!(parse_npy_f32(&data).is_err());
    }
}
