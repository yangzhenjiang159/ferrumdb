//! B+Tree 键的**保序编码**（阶段 6）。
//!
//! `ferrumdb-btree` 的 `PersistentBtree` 以 `(Vec<u8> key, Vec<u8> value)`
//! 工作，key 的字节序必须与值序一致，范围扫描才有意义。
//! 而 [`encode_row`]（`row.rs`）为紧凑存储设计（I64 用 LE、Bytes 用长度前缀），
//! **字节序 ≠ 值序**，不能直接当 B+Tree key 用。
//!
//! 本模块提供保序的单值编码，以及主键 / 二级索引复合键的拼装：
//!
//! | 变体 | 保序编码 |
//! |------|----------|
//! | `Null` | `0x00` |
//! | `I64(v)` | `0x01` ++ `((v as u64) ^ (1 << 63))` 的 8 字节 **big-endian** |
//! | `Bytes(b)` | `0x02` ++ 每字节 `0x00 → 0x00 0xFF`、其余原样，结尾 `0x00 0x00` |
//!
//! **类型标签**（首字节）保证同一列内所有值的编码**前缀无关**：
//! `i64::MIN` 翻转后是 `[0;8]`，若不加标签会与 `Null` 的 `[0x00]` 产生前缀冲突；
//! 加标签后 `Null=[0x00]`、`I64=[0x01…]`、`Bytes=[0x02…]` 互不为前缀，
//! 前缀扫描 + 前缀过滤因此是精确的。排序：`Null < I64 < Bytes`。
//!
//! 二级索引复合键 = 各索引列保序编码拼接 `∥` 主键保序编码（见 [`encode_secondary_key`]）。
//! 每列编码自定界（Null 1 字节 / I64 固定 9 字节 / Bytes 以 `0x00 0x00` 结尾），
//! 按 schema 解码无歧义，因此无需额外分隔符。
//!
//! 前缀扫描技巧：二级索引叶子 key = `index_key_enc ∥ pk_enc`。给定一个索引键前缀
//! `P` 和一个（主键）列类型，用 [`upper_bound`] 构造一个排他的字典序上界 `E`，
//! `scan_range(P, E)` 返回所有以 `P` 开头的 key，再按 `P` 前缀过滤（跨索引键的 key
//! 也可能落在 `[P, E)` 内）。这保证唯一索引探测 / `get_by_index` 不需要树内的前缀查询支持。

use crate::error::PageError;
use crate::row::{ColumnType, Row, Schema, Value};

/// `I64` 编码的类型标签字节。
pub const TAG_I64: u8 = 0x01;
/// `Bytes` 编码的类型标签字节。
pub const TAG_BYTES: u8 = 0x02;

/// 对单个 [`Value`] 做保序编码。
///
/// 编码结果同时满足：
/// - **保序**：`Value` 值序 ⟺ 字节序（相等则字节相等）
/// - **前缀无关**：任意两个不同值的编码互不为对方前缀（前缀扫描因此精确）
/// - **自定界**：能按 [`decode_key`] 从字节流中无歧义还原，且不留残余
///
/// 不依赖 `ColumnType`：编码仅由值变体决定。
pub fn encode_key(value: &Value) -> Vec<u8> {
    match value {
        Value::Null => vec![0x00],
        Value::I64(v) => {
            let flipped = (*v as u64) ^ (1u64 << 63);
            let mut out = Vec::with_capacity(9);
            out.push(TAG_I64);
            out.extend_from_slice(&flipped.to_be_bytes());
            out
        }
        Value::Bytes(b) => {
            let mut out = Vec::with_capacity(b.len() + 3);
            out.push(TAG_BYTES);
            for &byte in b {
                if byte == 0x00 {
                    // Escape a zero byte so it can never be confused with the
                    // terminator sequence (0x00 0x00) or a raw length byte.
                    out.push(0x00);
                    out.push(0xFF);
                } else {
                    out.push(byte);
                }
            }
            out.push(0x00);
            out.push(0x00);
            out
        }
    }
}

/// 对单个保序编码的字节解码为一个 [`Value`]。
///
/// 返回 `(value, consumed)`，`consumed` 是编码占用的字节数（用于按 schema 连续解析）。
///
/// 解码由**类型标签**驱动，`col_type` 用于校验一致性：
/// `Null` 标签出现在任意列类型；`I64`/`Bytes` 标签与 `col_type` 不符时返回错误
/// （`Any` 表示不校验）。
///
/// # Errors
///
/// - 输入为空、编码提前截断 ⇒ `PageError::EncodingError`
/// - `Bytes` 出现非法 escape（`0x00` 后跟非 `0x00`/`0xFF`）⇒ `PageError::EncodingError`
/// - 标签与 `col_type` 不一致 ⇒ `PageError::EncodingError`
pub fn decode_key(bytes: &[u8], col_type: ColumnType) -> Result<(Value, usize), PageError> {
    if bytes.is_empty() {
        return Err(PageError::EncodingError("key bytes empty".into()));
    }
    match bytes[0] {
        0x00 => {
            // NULL encoding is a single tag byte.
            Ok((Value::Null, 1))
        }
        TAG_I64 => {
            if col_type == ColumnType::Bytes {
                return Err(PageError::EncodingError(
                    "tag I64 but column type is Bytes".into(),
                ));
            }
            if bytes.len() < 1 + 8 {
                return Err(PageError::EncodingError(format!(
                    "i64 key needs 9 bytes, got {}",
                    bytes.len()
                )));
            }
            let mut buf = [0u8; 8];
            buf.copy_from_slice(&bytes[1..1 + 8]);
            let flipped = u64::from_be_bytes(buf);
            Ok((Value::I64((flipped ^ (1u64 << 63)) as i64), 9))
        }
        TAG_BYTES => {
            if col_type == ColumnType::I64 {
                return Err(PageError::EncodingError(
                    "tag Bytes but column type is I64".into(),
                ));
            }
            let mut out = Vec::new();
            let mut i = 1usize; // skip tag
            loop {
                if i >= bytes.len() {
                    return Err(PageError::EncodingError(
                        "bytes key truncated before terminator".into(),
                    ));
                }
                if bytes[i] == 0x00 {
                    if i + 1 >= bytes.len() {
                        return Err(PageError::EncodingError(
                            "bytes key truncated inside escape".into(),
                        ));
                    }
                    match bytes[i + 1] {
                        0x00 => return Ok((Value::Bytes(out), i + 2)), // terminator
                        0xFF => {
                            out.push(0x00);
                            i += 2;
                        }
                        _ => {
                            return Err(PageError::EncodingError(format!(
                                "invalid bytes escape: 0x00 0x{:02X}",
                                bytes[i + 1]
                            )))
                        }
                    }
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
        }
        other => Err(PageError::EncodingError(format!(
            "unknown key tag: 0x{other:02X}"
        ))),
    }
}

/// 构造某列类型的**排他**字典序上界编码：所有该类型合法值的保序编码都 `<` 该上界。
///
/// 用于 `scan_range(start, end)` 的开区间上界：
/// - `I64`：所有编码以 `0x01` 开头，上界取 `0x02`（`Bytes` 标签），恒大于任意 `I64` 编码。
/// - `Bytes`：编码变长、无有限最大长度。返回一个文档化的大上界
///   `[0xFF; UPPER_BOUND_BYTES_LEN]`；扫描方需自行按前缀过滤，且数据行的
///   pk 编码长度在本阶段的现实规模内不会超出。若未来需要精确语义，可改为
///   "扫描到第一个不匹配前缀即停"的惰性迭代。
pub fn upper_bound(col_type: ColumnType) -> Vec<u8> {
    match col_type {
        ColumnType::I64 => vec![TAG_BYTES],
        ColumnType::Bytes => vec![0xFF; UPPER_BOUND_BYTES_LEN],
        ColumnType::Any => vec![0xFF; UPPER_BOUND_BYTES_LEN],
    }
}

/// [`upper_bound`] 对 `Bytes` 列使用的上界长度（足够覆盖阶段 6 的现实数据规模）。
pub const UPPER_BOUND_BYTES_LEN: usize = 64;

/// 返回 `bytes` 的字典序**后继**：恰好大于所有以 `bytes` 为前缀的字节串的最小上界。
///
/// 用于前缀扫描 `scan_range(bytes, successor(bytes))`，精确返回所有以 `bytes` 开头
/// 的 key（不会漏掉、也不会混入非该前缀的 key）。
///
/// 算法：找到最后一个 `< 0xFF` 的字节并加一；若全部为 `0xFF` 则不存在有限后继，返回
/// `None`（对真实索引键编码——首字节必为类型标签 `0x00/0x01/0x02`——不可能发生）。
pub fn successor(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut i = bytes.len();
    while i > 0 && bytes[i - 1] == 0xFF {
        i -= 1;
    }
    if i == 0 {
        return None;
    }
    let mut out = bytes[..i].to_vec();
    let last = out.len() - 1;
    out[last] += 1;
    Some(out)
}

/// 从一行中取主键列的保序编码，作为聚簇 B+Tree 的 key。
///
/// # Errors
///
/// - 行值数量与 schema 列数不一致 ⇒ `PageError::EncodingError`
/// - schema 未声明主键 ⇒ `PageError::EncodingError`
pub fn encode_pk(row: &Row, schema: &Schema) -> Result<Vec<u8>, PageError> {
    if row.values.len() != schema.columns.len() {
        return Err(PageError::EncodingError(format!(
            "row has {} values but schema expects {}",
            row.values.len(),
            schema.columns.len()
        )));
    }
    let Some(pk_idx) = schema.primary_key else {
        return Err(PageError::EncodingError(
            "schema has no primary key, cannot encode pk".into(),
        ));
    };
    Ok(encode_key(&row.values[pk_idx]))
}

/// 把主键列类型从 schema 中取出（用于解码二级索引 value 中的 pk 字节）。
///
/// # Errors
///
/// - schema 未声明主键 ⇒ `PageError::EncodingError`
pub fn primary_key_type(schema: &Schema) -> Result<ColumnType, PageError> {
    schema
        .primary_key
        .map(|idx| schema.types[idx])
        .ok_or_else(|| PageError::EncodingError("schema has no primary key".into()))
}

/// 将多个索引列的值编码为前缀字节串（不追加 pk）。
///
/// 供二级索引 insert（全部索引列）与 get_by_index / scan_index（前导列前缀）使用。
/// 每列编码自定界，拼接后按 schema 解码无歧义。
pub fn encode_index_key(values: &[Value]) -> Vec<u8> {
    let mut out = Vec::new();
    for v in values {
        out.extend_from_slice(&encode_key(v));
    }
    out
}

/// 二级索引叶子 key：`index_key_enc ∥ pk_enc`。
///
/// 主键作为最后一个自定界后缀拼入，使 (index_key, pk) 整体有序：
/// 同一索引键的多行按 pk 有序；唯一/非唯一共用同一结构。
pub fn encode_secondary_key(index_values: &[Value], pk_value: &Value) -> Vec<u8> {
    let mut out = encode_index_key(index_values);
    out.extend_from_slice(&encode_key(pk_value));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i64_order_is_preserved() {
        let mut values = [i64::MIN, -1000, -1, 0, 1, 42, i64::MAX];
        let mut encs: Vec<Vec<u8>> = values.iter().map(|v| encode_key(&Value::I64(*v))).collect();
        // Sanity: encodings are distinct 8-byte big-endian.
        assert!(encs.windows(2).all(|w| w[0] < w[1]));
        // Sorting the values must sort the encodings identically.
        values.sort_unstable();
        encs.sort_unstable();
        let expected: Vec<Vec<u8>> =
            values.iter().map(|v| encode_key(&Value::I64(*v))).collect();
        assert_eq!(encs, expected);
    }

    #[test]
    fn i64_extremes_round_trip() {
        for v in [i64::MIN, i64::MAX, 0, -1, 1] {
            let enc = encode_key(&Value::I64(v));
            assert_eq!(enc.len(), 9);
            assert_eq!(enc[0], TAG_I64);
            let (decoded, consumed) = decode_key(&enc, ColumnType::I64).unwrap();
            assert_eq!(decoded, Value::I64(v));
            assert_eq!(consumed, 9);
        }
    }

    #[test]
    fn bytes_escape_and_terminator() {
        // Empty bytes → tag + terminator only.
        assert_eq!(
            encode_key(&Value::Bytes(vec![])),
            vec![TAG_BYTES, 0x00, 0x00]
        );
        // A zero byte becomes 0x00 0xFF.
        assert_eq!(
            encode_key(&Value::Bytes(vec![0x00])),
            vec![TAG_BYTES, 0x00, 0xFF, 0x00, 0x00]
        );
        // Regular bytes pass through unchanged (plus tag + terminator).
        assert_eq!(
            encode_key(&Value::Bytes(b"ab".to_vec())),
            vec![TAG_BYTES, b'a', b'b', 0x00, 0x00]
        );
    }

    #[test]
    fn tagged_encoding_is_prefix_free_across_types() {
        // i64::MIN's flipped encoding is all zeros; with the tag, no value's
        // encoding may be a strict prefix of another's (critical for scans).
        let min = encode_key(&Value::I64(i64::MIN));
        assert_eq!(&min[1..], &[0u8; 8]);
        assert_eq!(&min[..1], &[TAG_I64]);
        // NULL = [0x00], i64 = [TAG_I64, ...]; neither is a prefix of the other.
        let null = encode_key(&Value::Null);
        assert_eq!(null, vec![0x00]);
        assert!(!min.starts_with(&null));
        assert!(!null.starts_with(&min));
        // Same-type values are also prefix-free.
        let a = encode_key(&Value::Bytes(b"a".to_vec()));
        let ab = encode_key(&Value::Bytes(b"ab".to_vec()));
        assert!(!ab.starts_with(&a));
        assert!(!a.starts_with(&ab));
    }

    #[test]
    fn bytes_order_is_preserved() {
        let vals = [
            Vec::new(),
            vec![0x00],
            vec![0x00, 0x00],
            vec![0x01],
            b"a".to_vec(),
            b"ab".to_vec(),
            b"b".to_vec(),
        ];
        let mut encs: Vec<Vec<u8>> = vals.iter().map(|v| encode_key(&Value::Bytes(v.clone()))).collect();
        encs.sort_unstable();
        let expected: Vec<Vec<u8>> =
            vals.iter().map(|v| encode_key(&Value::Bytes(v.clone()))).collect();
        assert_eq!(encs, expected);
    }

    #[test]
    fn bytes_round_trip_including_zeros() {
        let payload = vec![0u8, 1, 0, 2, 0xFF, 0, 255];
        let enc = encode_key(&Value::Bytes(payload.clone()));
        let (decoded, consumed) = decode_key(&enc, ColumnType::Bytes).unwrap();
        assert_eq!(decoded, Value::Bytes(payload));
        assert_eq!(consumed, enc.len());
    }

    #[test]
    fn null_round_trip_by_type() {
        for col in [ColumnType::I64, ColumnType::Bytes] {
            let enc = encode_key(&Value::Null);
            assert_eq!(enc, vec![0x00]);
            let (decoded, consumed) = decode_key(&enc, col).unwrap();
            assert_eq!(decoded, Value::Null);
            assert_eq!(consumed, 1);
        }
    }

    #[test]
    fn truncated_i64_and_bytes_errors() {
        // Tag only, no payload.
        assert!(matches!(
            decode_key(&[TAG_I64], ColumnType::I64),
            Err(PageError::EncodingError(_))
        ));
        // Bytes with a dangling escape byte.
        assert!(matches!(
            decode_key(&[TAG_BYTES, 0x00, 0xFE], ColumnType::Bytes),
            Err(PageError::EncodingError(_))
        ));
        // Bytes truncated mid-escape: tag + lone raw byte with no terminator.
        assert!(matches!(
            decode_key(&[TAG_BYTES, 0x01], ColumnType::Bytes),
            Err(PageError::EncodingError(_))
        ));
    }

    #[test]
    fn tag_type_mismatch_errors() {
        // I64 tag in a Bytes-typed column.
        assert!(matches!(
            decode_key(&encode_key(&Value::I64(1)), ColumnType::Bytes),
            Err(PageError::EncodingError(_))
        ));
        // Bytes tag in an I64-typed column.
        assert!(matches!(
            decode_key(&encode_key(&Value::Bytes(vec![1])), ColumnType::I64),
            Err(PageError::EncodingError(_))
        ));
    }

    #[test]
    fn upper_bound_exceeds_all_i64_encodings() {
        let bound = upper_bound(ColumnType::I64);
        assert_eq!(bound, vec![TAG_BYTES]);
        // Even the maximum I64 encoding (i64::MAX) is below the bound.
        let max_enc = encode_key(&Value::I64(i64::MAX));
        assert!(max_enc.as_slice() < bound.as_slice());
        // And the minimum.
        let min_enc = encode_key(&Value::I64(i64::MIN));
        assert!(min_enc.as_slice() < bound.as_slice());
    }

    #[test]
    fn upper_bound_bytes_is_reasonable() {
        let bound = upper_bound(ColumnType::Bytes);
        assert!(bound.len() >= 8);
        // A moderately long Bytes pk encoding stays below the bound.
        let enc = encode_key(&Value::Bytes(vec![0xFF; 32]));
        assert!(enc.as_slice() < bound.as_slice());
    }

    #[test]
    fn successor_is_exclusive_prefix_bound() {
        // successor("ab") = "ac": 所有 "ab*" < "ac"。
        assert_eq!(successor(b"ab").unwrap(), b"ac".to_vec());
        // 末字节 0xFF：进位到前一个字节。
        assert_eq!(successor(b"a\xff").unwrap(), b"b".to_vec());
        // 全部 0xFF 无有限后继（调用方需兜底）。
        assert_eq!(successor(b"\xff"), None);
        assert_eq!(successor(b"\xff\xff"), None);
        // 任一以 bytes 为前缀的串都 < successor。
        for p in [b"ab".as_slice(), b"a\xff".as_slice(), b"abc".as_slice()] {
            let succ = successor(p).unwrap();
            let ext = [p, b"\xff\xff\xff"].concat();
            assert!(ext.as_slice() < succ.as_slice());
        }
    }

    #[test]
    fn prefix_scan_bound_captures_all_extensions() {
        // For an I64-pk secondary tree, the bound P ++ upper_bound(pk_type) is a strict
        // upper bound for every key that starts with P (regardless of P's tail bytes).
        for p in [vec![], vec![0x00], vec![0xFF], b"ab".to_vec(), vec![0xFF, 0xFF]] {
            let end = [p.clone(), upper_bound(ColumnType::I64)].concat();
            // Every key P ++ pk_enc must be < end.
            let pk_exts = [i64::MIN, 0, i64::MAX];
            for pk in pk_exts {
                let key = [p.clone(), encode_key(&Value::I64(pk))].concat();
                assert!(
                    key.as_slice() < end.as_slice(),
                    "key {:?} not below end {:?} for prefix {:?}",
                    key,
                    end,
                    p
                );
            }
        }
    }

    #[test]
    fn secondary_key_suffix_orders_by_pk() {
        let idx = [Value::I64(1)];
        let pk_low = Value::I64(10);
        let pk_high = Value::I64(20);
        let low = encode_secondary_key(&idx, &pk_low);
        let high = encode_secondary_key(&idx, &pk_high);
        assert!(low < high);
        // Same pk under different index keys orders by index key.
        let idx2 = [Value::I64(2)];
        assert!(encode_secondary_key(&idx, &pk_low) < encode_secondary_key(&idx2, &pk_low));
    }

    #[test]
    fn encode_pk_requires_primary_key() {
        let schema = Schema {
            columns: vec!["id".into()],
            types: vec![ColumnType::I64],
            primary_key: Some(0),
        };
        let row = Row { values: vec![Value::I64(7)] };
        assert_eq!(encode_pk(&row, &schema).unwrap(), encode_key(&Value::I64(7)));

        let no_pk = Schema {
            columns: vec!["id".into()],
            types: vec![ColumnType::I64],
            primary_key: None,
        };
        assert!(matches!(
            encode_pk(&row, &no_pk),
            Err(PageError::EncodingError(_))
        ));
    }
}
