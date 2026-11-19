//! Just enough of Python's pickle format to read a PyTorch checkpoint: plain
//! values, dicts, and tensors that point into the checkpoint's data files.
//! Nothing is ever executed; unknown callables are kept as inert values.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Bytes(Vec<u8>),
    Tuple(Vec<Value>),
    List(Vec<Value>),
    Dict(Vec<(Value, Value)>),
    Global {
        module: String,
        name: String,
    },
    /// A call this reader doesn't model, kept as data.
    Object {
        callable: Box<Value>,
        args: Box<Value>,
    },
    Persistent(Box<Value>),
    Tensor(TensorRef),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Dict(items) => items
                .iter()
                .find(|(k, _)| matches!(k, Value::Str(s) if s == key))
                .map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Bool(b) => Some(*b as i64),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DType {
    F16,
    BF16,
    F32,
    F64,
}

impl DType {
    pub fn size(self) -> usize {
        match self {
            DType::F16 | DType::BF16 => 2,
            DType::F32 => 4,
            DType::F64 => 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TensorRef {
    /// Name of the data file inside the checkpoint's `data/` folder.
    pub storage: String,
    pub dtype: DType,
    /// In elements, not bytes.
    pub offset: usize,
    pub shape: Vec<usize>,
    pub stride: Vec<usize>,
}

pub fn parse(data: &[u8]) -> anyhow::Result<Value> {
    let mut r = Reader { data, pos: 0 };
    let mut stack: Vec<Value> = Vec::new();
    let mut marks: Vec<usize> = Vec::new();
    let mut memo: HashMap<u32, Value> = HashMap::new();

    fn pop(stack: &mut Vec<Value>) -> anyhow::Result<Value> {
        stack
            .pop()
            .ok_or_else(|| anyhow::anyhow!("pickle stack underflow"))
    }
    fn top(stack: &mut [Value]) -> anyhow::Result<&mut Value> {
        stack
            .last_mut()
            .ok_or_else(|| anyhow::anyhow!("pickle stack underflow"))
    }
    fn from_mark(stack: &mut Vec<Value>, marks: &mut Vec<usize>) -> anyhow::Result<Vec<Value>> {
        let m = marks
            .pop()
            .ok_or_else(|| anyhow::anyhow!("pickle mark missing"))?;
        anyhow::ensure!(m <= stack.len(), "pickle mark past the stack");
        Ok(stack.split_off(m))
    }

    loop {
        let op = r.byte()?;
        match op {
            0x80 => {
                r.byte()?; // PROTO
            }
            0x95 => {
                r.take(8)?; // FRAME
            }
            b'}' => stack.push(Value::Dict(Vec::new())),
            b']' => stack.push(Value::List(Vec::new())),
            b')' => stack.push(Value::Tuple(Vec::new())),
            b'(' => marks.push(stack.len()),
            b'q' => {
                let i = r.byte()? as u32;
                memo.insert(i, top(&mut stack)?.clone());
            }
            b'r' => {
                let i = r.u32()?;
                memo.insert(i, top(&mut stack)?.clone());
            }
            0x94 => {
                let i = memo.len() as u32;
                memo.insert(i, top(&mut stack)?.clone());
            }
            b'h' => {
                let i = r.byte()? as u32;
                stack.push(
                    memo.get(&i)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("pickle memo {i} missing"))?,
                );
            }
            b'j' => {
                let i = r.u32()?;
                stack.push(
                    memo.get(&i)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("pickle memo {i} missing"))?,
                );
            }
            b'X' => {
                let n = r.u32()? as usize;
                stack.push(Value::Str(String::from_utf8_lossy(r.take(n)?).into_owned()));
            }
            0x8c => {
                let n = r.byte()? as usize;
                stack.push(Value::Str(String::from_utf8_lossy(r.take(n)?).into_owned()));
            }
            b'U' => {
                let n = r.byte()? as usize;
                stack.push(Value::Str(r.take(n)?.iter().map(|&b| b as char).collect()));
            }
            b'T' => {
                let n = r.u32()? as usize;
                stack.push(Value::Str(r.take(n)?.iter().map(|&b| b as char).collect()));
            }
            b'C' => {
                let n = r.byte()? as usize;
                stack.push(Value::Bytes(r.take(n)?.to_vec()));
            }
            b'B' => {
                let n = r.u32()? as usize;
                stack.push(Value::Bytes(r.take(n)?.to_vec()));
            }
            b'c' => {
                let module = r.line()?;
                let name = r.line()?;
                stack.push(Value::Global { module, name });
            }
            0x93 => {
                let name = pop(&mut stack)?;
                let module = pop(&mut stack)?;
                match (module, name) {
                    (Value::Str(module), Value::Str(name)) => {
                        stack.push(Value::Global { module, name })
                    }
                    _ => anyhow::bail!("pickle STACK_GLOBAL needs two strings"),
                }
            }
            b'K' => stack.push(Value::Int(r.byte()? as i64)),
            b'M' => stack.push(Value::Int(u16::from_le_bytes(r.array()?) as i64)),
            b'J' => stack.push(Value::Int(i32::from_le_bytes(r.array()?) as i64)),
            0x8a => {
                let n = r.byte()? as usize;
                let bytes = r.take(n)?;
                anyhow::ensure!(n <= 8, "pickle integer too large");
                let mut v: i64 = 0;
                for (i, &b) in bytes.iter().enumerate() {
                    v |= (b as i64) << (8 * i);
                }
                if n > 0 && n < 8 && bytes[n - 1] & 0x80 != 0 {
                    v -= 1i64 << (8 * n);
                }
                stack.push(Value::Int(v));
            }
            b'G' => stack.push(Value::Float(f64::from_be_bytes(r.array()?))),
            b'N' => stack.push(Value::None),
            0x88 => stack.push(Value::Bool(true)),
            0x89 => stack.push(Value::Bool(false)),
            b't' => {
                let items = from_mark(&mut stack, &mut marks)?;
                stack.push(Value::Tuple(items));
            }
            0x85 => {
                let a = pop(&mut stack)?;
                stack.push(Value::Tuple(vec![a]));
            }
            0x86 => {
                let b = pop(&mut stack)?;
                let a = pop(&mut stack)?;
                stack.push(Value::Tuple(vec![a, b]));
            }
            0x87 => {
                let c = pop(&mut stack)?;
                let b = pop(&mut stack)?;
                let a = pop(&mut stack)?;
                stack.push(Value::Tuple(vec![a, b, c]));
            }
            b'Q' => {
                let pid = pop(&mut stack)?;
                stack.push(Value::Persistent(Box::new(pid)));
            }
            b'R' | 0x81 => {
                let args = pop(&mut stack)?;
                let callable = pop(&mut stack)?;
                stack.push(reduce(callable, args)?);
            }
            b's' => {
                let v = pop(&mut stack)?;
                let k = pop(&mut stack)?;
                insert(top(&mut stack)?, k, v);
            }
            b'u' => {
                let items = from_mark(&mut stack, &mut marks)?;
                let target = top(&mut stack)?;
                let mut it = items.into_iter();
                while let (Some(k), Some(v)) = (it.next(), it.next()) {
                    insert(target, k, v);
                }
            }
            b'a' => {
                let v = pop(&mut stack)?;
                if let Value::List(l) = top(&mut stack)? {
                    l.push(v);
                }
            }
            b'e' => {
                let items = from_mark(&mut stack, &mut marks)?;
                if let Value::List(l) = top(&mut stack)? {
                    l.extend(items);
                }
            }
            b'b' => {
                // BUILD sets attributes (like an OrderedDict's `_metadata`). Not needed.
                pop(&mut stack)?;
            }
            b'.' => return pop(&mut stack),
            other => anyhow::bail!(
                "unsupported pickle opcode 0x{other:02x} at byte {}",
                r.pos - 1
            ),
        }
    }
}

fn insert(target: &mut Value, k: Value, v: Value) {
    if let Value::Dict(items) = target {
        items.push((k, v));
    }
}

fn reduce(callable: Value, args: Value) -> anyhow::Result<Value> {
    if let Value::Global { module, name } = &callable {
        match (module.as_str(), name.as_str()) {
            ("collections", "OrderedDict") => return Ok(Value::Dict(Vec::new())),
            ("torch._utils", "_rebuild_tensor_v2" | "_rebuild_tensor") => return tensor(&args),
            _ => {}
        }
    }
    Ok(Value::Object {
        callable: Box::new(callable),
        args: Box::new(args),
    })
}

/// `_rebuild_tensor_v2(storage, storage_offset, size, stride, ...)`, where the
/// storage is the persistent id `('storage', torch.HalfStorage, key, location, numel)`.
fn tensor(args: &Value) -> anyhow::Result<Value> {
    let Value::Tuple(a) = args else {
        anyhow::bail!("tensor arguments are not a tuple");
    };
    anyhow::ensure!(a.len() >= 4, "tensor arguments are incomplete");
    let Value::Persistent(pid) = &a[0] else {
        anyhow::bail!("tensor storage is not a persistent id");
    };
    let Value::Tuple(p) = pid.as_ref() else {
        anyhow::bail!("tensor storage id is not a tuple");
    };
    anyhow::ensure!(p.len() >= 3, "tensor storage id is incomplete");
    let dtype = match &p[1] {
        Value::Global { name, .. } => match name.as_str() {
            "HalfStorage" => DType::F16,
            "BFloat16Storage" => DType::BF16,
            "FloatStorage" => DType::F32,
            "DoubleStorage" => DType::F64,
            other => anyhow::bail!("unsupported tensor storage {other}"),
        },
        _ => anyhow::bail!("tensor storage type missing"),
    };
    let storage = match &p[2] {
        Value::Str(s) => s.clone(),
        _ => anyhow::bail!("tensor storage key missing"),
    };
    let ints = |v: &Value| -> anyhow::Result<Vec<usize>> {
        match v {
            Value::Tuple(items) => items
                .iter()
                .map(|i| {
                    i.as_int()
                        .map(|n| n as usize)
                        .ok_or_else(|| anyhow::anyhow!("bad tensor shape"))
                })
                .collect(),
            _ => anyhow::bail!("tensor shape is not a tuple"),
        }
    };
    Ok(Value::Tensor(TensorRef {
        storage,
        dtype,
        offset: a[1].as_int().unwrap_or(0) as usize,
        shape: ints(&a[2])?,
        stride: ints(&a[3])?,
    }))
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> anyhow::Result<&'a [u8]> {
        anyhow::ensure!(self.pos + n <= self.data.len(), "pickle data ends early");
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn byte(&mut self) -> anyhow::Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn array<const N: usize>(&mut self) -> anyhow::Result<[u8; N]> {
        Ok(self.take(N)?.try_into()?)
    }

    fn u32(&mut self) -> anyhow::Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn line(&mut self) -> anyhow::Result<String> {
        let rest = &self.data[self.pos..];
        let end = rest
            .iter()
            .position(|&b| b == b'\n')
            .ok_or_else(|| anyhow::anyhow!("pickle line has no end"))?;
        self.pos += end + 1;
        Ok(String::from_utf8_lossy(&rest[..end]).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `pickle.dumps({'dims': {'n_mels': 80}, 'ok': True, 'x': -5, 'big': 2**40, 'f': 1.5}, protocol=2)`
    #[test]
    fn reads_plain_values() {
        let data: &[u8] = b"\x80\x02}q\x00(X\x04\x00\x00\x00dimsq\x01}q\x02X\x06\x00\x00\x00n_melsq\x03KPsX\x02\x00\x00\x00okq\x04\x88X\x01\x00\x00\x00xq\x05J\xfb\xff\xff\xffX\x03\x00\x00\x00bigq\x06\x8a\x06\x00\x00\x00\x00\x00\x01X\x01\x00\x00\x00fq\x07G?\xf8\x00\x00\x00\x00\x00\x00u.";
        let v = parse(data).unwrap();
        assert_eq!(
            v.get("dims")
                .and_then(|d| d.get("n_mels"))
                .and_then(Value::as_int),
            Some(80)
        );
        assert_eq!(v.get("ok"), Some(&Value::Bool(true)));
        assert_eq!(v.get("x").and_then(Value::as_int), Some(-5));
        assert_eq!(v.get("big").and_then(Value::as_int), Some(1 << 40));
        assert_eq!(v.get("f"), Some(&Value::Float(1.5)));
    }

    #[test]
    fn reads_a_torch_tensor_reference() {
        // OrderedDict([('w', _rebuild_tensor_v2(('storage', torch.HalfStorage, '0', 'cpu', 6), 0, (2, 3), (3, 1), False, OrderedDict()))])
        let mut data = Vec::new();
        data.extend_from_slice(
            b"\x80\x02ccollections\nOrderedDict\nq\x00)Rq\x01X\x01\x00\x00\x00wq\x02",
        );
        data.extend_from_slice(
            b"ctorch._utils\n_rebuild_tensor_v2\nq\x03((X\x07\x00\x00\x00storageq\x04",
        );
        data.extend_from_slice(b"ctorch\nHalfStorage\nq\x05X\x01\x00\x00\x000q\x06X\x03\x00\x00\x00cpuq\x07K\x06tq\x08Q");
        data.extend_from_slice(
            b"K\x00K\x02K\x03\x86q\tK\x03K\x01\x86q\n\x89h\x00)Rq\x0btq\x0cRq\rs.",
        );
        let v = parse(&data).unwrap();
        let Some(Value::Tensor(t)) = v.get("w") else {
            panic!("expected a tensor, got {v:?}");
        };
        assert_eq!(t.storage, "0");
        assert_eq!(t.dtype, DType::F16);
        assert_eq!(t.shape, vec![2, 3]);
        assert_eq!(t.stride, vec![3, 1]);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse(b"\x80\x02\xff").is_err());
        assert!(parse(b"").is_err());
    }
}
