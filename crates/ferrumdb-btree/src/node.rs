//! B+Tree 节点类型。

use std::marker::PhantomData;

/// B+Tree 的阶（每个节点最多 `ORDER` 个 key；内部节点最多 `ORDER` 个 child）。
pub const ORDER: usize = 64;

/// 节点分裂后允许的最小 key 数。下溢时调用方需要合并或重分配。
pub const MIN_KEYS: usize = ORDER / 2;

/// B+Tree 节点。
///
/// 内部节点：`keys.len() == children.len() - 1`，每个 `keys[i]` 是
/// `children[i]` 与 `children[i+1]` 之间的分隔键（边界 key 方案）。
///
/// 叶子节点：`keys[i]` 与 `values[i]` 一一对应；`next` 形成单向链表
/// （v2 可升级为双向）。
pub enum Node<K, V> {
    /// 内部节点。
    Internal {
        /// 分隔键（升序）。
        keys: Vec<K>,
        /// 子节点指针；`children.len() == keys.len() + 1`。
        children: Vec<Box<Node<K, V>>>,
    },
    /// 叶子节点。
    Leaf {
        /// 主键（升序）。
        keys: Vec<K>,
        /// 值（与 keys 一一对应）。
        values: Vec<V>,
        /// 链表中的下一个叶子。`None` 表示链表末尾。
        /// 用裸指针 + PhantomData 避免 Rc/Arc 的借用限制，同时保持 Send + Sync 派生。
        next: Option<*mut Node<K, V>>,
        /// PhantomData 让编译器把 `Node` 当作拥有 `Box<Node<K, V>>` 的形式处理。
        _marker: PhantomData<Box<Node<K, V>>>,
    },
}

/// 在已排序的 `keys` 中二分查找 `key` 的插入位置（第一个 `>= key` 的位置）。
pub fn lower_bound<K: Ord>(keys: &[K], key: &K) -> usize {
    keys.binary_search(key).unwrap_or_else(|i| i)
}

impl<K, V> Node<K, V> {
    /// 新建空叶子。
    pub fn new_leaf() -> Box<Self> {
        Box::new(Node::Leaf {
            keys: Vec::new(),
            values: Vec::new(),
            next: None,
            _marker: PhantomData,
        })
    }

    /// 新建空内部节点。
    pub fn new_internal() -> Box<Self> {
        Box::new(Node::Internal {
            keys: Vec::new(),
            children: Vec::new(),
        })
    }

    /// 是否为叶子。
    pub fn is_leaf(&self) -> bool {
        matches!(self, Node::Leaf { .. })
    }

    /// 节点持有的 key 数量。
    pub fn key_len(&self) -> usize {
        match self {
            Node::Internal { keys, .. } => keys.len(),
            Node::Leaf { keys, .. } => keys.len(),
        }
    }

    /// 是否需要分裂（key 数达到上限）。
    pub fn needs_split(&self) -> bool {
        self.key_len() >= ORDER
    }
}
