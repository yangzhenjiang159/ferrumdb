//! 节点 ↔ Page 二进制序列化。

use ferrumdb_page::Page;
use ferrumdb_space::PageSource;

use crate::error::BTreeError;
use crate::node::ORDER;

/// 节点 kind 字节。
pub const KIND_INTERNAL: u8 = 0;
/// 叶子节点 kind 字节。
pub const KIND_LEAF: u8 = 1;

/// 节点页 user_data 布局：
///
/// ```text
/// +--------------------+
/// | kind: u8           |   0=Internal, 1=Leaf
/// | reserved: 3 bytes  |
/// | key_count: u16 LE  |   最多 ORDER
/// | reserved: 2 bytes  |
/// +--------------------+   header = 8 bytes
/// | keys: [len:u32][bytes] * key_count
/// | (Internal only) children: [count:u32][page_id:u32]*count
/// | (Leaf only)     values: [len:u32][bytes] * key_count
/// | (Leaf only)     next_leaf: [is_some:u8][page_id:u32]
/// +--------------------+
/// ```
const HEADER_BYTES: usize = 8;
const KIND_OFFSET: usize = 0;
const KEY_COUNT_OFFSET: usize = 4;

/// 把节点编码进 `Page` 的 user_data（覆盖）。
pub fn encode_node_to_page(
    page: &mut Page,
    node: EncodedNode<'_>,
) -> Result<(), BTreeError> {
    let ud = page.user_data_mut();
    // Capture key count for later use.
    let key_count = node.keys.len();
    // Validate key_count.
    if key_count > ORDER {
        return Err(BTreeError::TooManyKeys {
            got: node.keys.len(),
            max: ORDER,
        });
    }
    // Clear and write header.
    for b in ud.iter_mut() {
        *b = 0;
    }
    ud[KIND_OFFSET] = node.kind;
    let kc = (node.keys.len() as u16).to_le_bytes();
    ud[KEY_COUNT_OFFSET..KEY_COUNT_OFFSET + 2].copy_from_slice(&kc);

    // Cursor for variable-length data.
    let mut cur = HEADER_BYTES;

    // Keys
    let key_count = node.keys.len();
    for k in node.keys {
        let bytes = k;
        let len = bytes.len() as u32;
        ud[cur..cur + 4].copy_from_slice(&len.to_le_bytes());
        cur += 4;
        ud[cur..cur + bytes.len()].copy_from_slice(bytes);
        cur += bytes.len();
    }

    // Children (Internal) or values (Leaf)
    match node.kind {
        KIND_INTERNAL => {
            let children = node.children.ok_or(BTreeError::ArityMismatch {
                keys: key_count,
                children: 0,
                values: 0,
            })?;
            // Arity check: children == keys + 1
            if children.len() != key_count + 1 {
                return Err(BTreeError::ArityMismatch {
                    keys: key_count,
                    children: children.len(),
                    values: 0,
                });
            }
            let count = children.len() as u32;
            ud[cur..cur + 4].copy_from_slice(&count.to_le_bytes());
            cur += 4;
            for c in children {
                ud[cur..cur + 4].copy_from_slice(&c.to_le_bytes());
                cur += 4;
            }
        }
        KIND_LEAF => {
            let values = node.values.ok_or(BTreeError::ArityMismatch {
                keys: key_count,
                children: 0,
                values: 0,
            })?;
            // Arity check: values == keys
            if values.len() != key_count {
                return Err(BTreeError::ArityMismatch {
                    keys: key_count,
                    children: 0,
                    values: values.len(),
                });
            }
            for v in values {
                let bytes = v;
                let len = bytes.len() as u32;
                ud[cur..cur + 4].copy_from_slice(&len.to_le_bytes());
                cur += 4;
                ud[cur..cur + bytes.len()].copy_from_slice(bytes);
                cur += bytes.len();
            }
            // Next leaf pointer.
            match node.next_leaf {
                Some(id) => {
                    ud[cur] = 1;
                    ud[cur + 1..cur + 5].copy_from_slice(&id.to_le_bytes());
                }
                None => {
                    ud[cur] = 0;
                }
            }
        }
        other => return Err(BTreeError::InvalidNodeKind(other)),
    }
    Ok(())
}

/// 从 `Page` 解码节点。
pub fn decode_node_from_page(page: &Page) -> Result<DecodedNode, BTreeError> {
    let ud = page.user_data();
    if ud.len() < HEADER_BYTES {
        return Err(BTreeError::ArityMismatch {
            keys: 0,
            children: 0,
            values: 0,
        });
    }
    let kind = ud[KIND_OFFSET];
    let key_count = u16::from_le_bytes([ud[KEY_COUNT_OFFSET], ud[KEY_COUNT_OFFSET + 1]]) as usize;
    if key_count > ORDER {
        return Err(BTreeError::TooManyKeys {
            got: key_count,
            max: ORDER,
        });
    }

    let mut cur = HEADER_BYTES;
    let mut keys = Vec::with_capacity(key_count);
    for _ in 0..key_count {
        if cur + 4 > ud.len() {
            return Err(BTreeError::ArityMismatch {
                keys: keys.len(),
                children: 0,
                values: 0,
            });
        }
        let len = u32::from_le_bytes([ud[cur], ud[cur + 1], ud[cur + 2], ud[cur + 3]]) as usize;
        cur += 4;
        if cur + len > ud.len() {
            return Err(BTreeError::ArityMismatch {
                keys: keys.len(),
                children: 0,
                values: 0,
            });
        }
        keys.push(ud[cur..cur + len].to_vec());
        cur += len;
    }

    match kind {
        KIND_INTERNAL => {
            if cur + 4 > ud.len() {
                return Err(BTreeError::ArityMismatch {
                    keys: keys.len(),
                    children: 0,
                    values: 0,
                });
            }
            let child_count =
                u32::from_le_bytes([ud[cur], ud[cur + 1], ud[cur + 2], ud[cur + 3]]) as usize;
            cur += 4;
            if child_count > ORDER + 1 || cur + child_count * 4 > ud.len() {
                return Err(BTreeError::ArityMismatch {
                    keys: keys.len(),
                    children: child_count,
                    values: 0,
                });
            }
            let mut children = Vec::with_capacity(child_count);
            for _ in 0..child_count {
                let id = u32::from_le_bytes([ud[cur], ud[cur + 1], ud[cur + 2], ud[cur + 3]]);
                cur += 4;
                children.push(id);
            }
            Ok(DecodedNode::Internal { keys, children })
        }
        KIND_LEAF => {
            let mut values = Vec::with_capacity(key_count);
            for _ in 0..key_count {
                if cur + 4 > ud.len() {
                    return Err(BTreeError::ArityMismatch {
                        keys: keys.len(),
                        children: 0,
                        values: values.len(),
                    });
                }
                let len = u32::from_le_bytes([ud[cur], ud[cur + 1], ud[cur + 2], ud[cur + 3]]) as usize;
                cur += 4;
                if cur + len > ud.len() {
                    return Err(BTreeError::ArityMismatch {
                        keys: keys.len(),
                        children: 0,
                        values: values.len(),
                    });
                }
                values.push(ud[cur..cur + len].to_vec());
                cur += len;
            }
            let next_leaf = if cur + 5 <= ud.len() && ud[cur] != 0 {
                let id = u32::from_le_bytes([ud[cur + 1], ud[cur + 2], ud[cur + 3], ud[cur + 4]]);
                Some(id)
            } else {
                None
            };
            Ok(DecodedNode::Leaf {
                keys,
                values,
                next_leaf,
            })
        }
        other => Err(BTreeError::InvalidNodeKind(other)),
    }
}

/// 写入节点的中间表示（key/value 以字节序列承载，避免泛型与生命周期问题）。
pub struct EncodedNode<'a> {
    /// `KIND_INTERNAL` 或 `KIND_LEAF`。
    pub kind: u8,
    /// key 字节数组（升序）。
    pub keys: Vec<&'a [u8]>,
    /// 内部节点：children page_id 数组（长度 = keys + 1）。
    pub children: Option<Vec<u32>>,
    /// 叶子节点：value 字节数组（长度 = keys）。
    pub values: Option<Vec<&'a [u8]>>,
    /// 叶子节点：next leaf page id（`None` 表示链表末尾）。
    pub next_leaf: Option<u32>,
}

/// 从 Page 解码出的节点。
#[derive(Debug, Clone)]
pub enum DecodedNode {
    /// 内部节点。
    Internal {
        /// key 字节数组。
        keys: Vec<Vec<u8>>,
        /// 子节点 page_id。
        children: Vec<u32>,
    },
    /// 叶子节点。
    Leaf {
        /// key 字节数组。
        keys: Vec<Vec<u8>>,
        /// value 字节数组。
        values: Vec<Vec<u8>>,
        /// next leaf page id。
        next_leaf: Option<u32>,
    },
}

// Convenience: from Source + page_id
impl DecodedNode {
    /// 便捷：从 PageSource + page_id 读 + 解码。
    pub fn load<S: PageSource + ?Sized>(source: &mut S, page_id: u32) -> Result<Self, BTreeError> {
        let page = source.read_page(page_id)?;
        decode_node_from_page(&page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdb_page::PageType;

    #[test]
    fn leaf_round_trip() {
        let mut page = Page::new(7, PageType::Index);
        let node = EncodedNode {
            kind: KIND_LEAF,
            keys: vec![b"alpha".as_ref(), b"bravo".as_ref(), b"charlie".as_ref()],
            children: None,
            values: Some(vec![b"v1".as_ref(), b"v2".as_ref(), b"v3".as_ref()]),
            next_leaf: Some(42),
        };
        encode_node_to_page(&mut page, node).unwrap();
        let decoded = decode_node_from_page(&page).unwrap();
        match decoded {
            DecodedNode::Leaf { keys, values, next_leaf } => {
                assert_eq!(keys, vec![b"alpha".to_vec(), b"bravo".to_vec(), b"charlie".to_vec()]);
                assert_eq!(values, vec![b"v1".to_vec(), b"v2".to_vec(), b"v3".to_vec()]);
                assert_eq!(next_leaf, Some(42));
            }
            _ => panic!("expected leaf"),
        }
    }

    #[test]
    fn internal_round_trip() {
        let mut page = Page::new(3, PageType::Index);
        let node = EncodedNode {
            kind: KIND_INTERNAL,
            keys: vec![b"k1".as_ref(), b"k2".as_ref()],
            children: Some(vec![10, 20, 30]),
            values: None,
            next_leaf: None,
        };
        encode_node_to_page(&mut page, node).unwrap();
        let decoded = decode_node_from_page(&page).unwrap();
        match decoded {
            DecodedNode::Internal { keys, children } => {
                assert_eq!(keys, vec![b"k1".to_vec(), b"k2".to_vec()]);
                assert_eq!(children, vec![10, 20, 30]);
            }
            _ => panic!("expected internal"),
        }
    }

    #[test]
    fn arity_mismatch_returns_error() {
        let mut page = Page::new(1, PageType::Index);
        let node = EncodedNode {
            kind: KIND_LEAF,
            keys: vec![b"k1".as_ref()],
            children: None,
            values: Some(vec![b"v1".as_ref(), b"v2".as_ref()]), // 2 values for 1 key
            next_leaf: None,
        };
        let err = encode_node_to_page(&mut page, node);
        assert!(matches!(err, Err(BTreeError::ArityMismatch { .. })));
    }

    #[test]
    fn invalid_kind_returns_error() {
        let page = Page::new(1, PageType::Index);
        let mut page = page;
        page.user_data_mut()[0] = 99; // bogus kind
        let err = decode_node_from_page(&page);
        assert!(matches!(err, Err(BTreeError::InvalidNodeKind(99))));
    }
}
