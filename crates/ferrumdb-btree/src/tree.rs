//! 内存 B+Tree 主结构。

use std::marker::PhantomData;

use crate::error::BTreeError;
use crate::node::{Node, MIN_KEYS, ORDER};

/// B+Tree 的分裂结果：上推到父节点的 key 与新建的右子树。
pub struct Split<K, V> {
    /// 上推的 key（边界 key；插入到父节点）。
    pub key: K,
    /// 新建的右子树（插入到父节点 keys 对应位置右侧）。
    pub right: Box<Node<K, V>>,
}

/// 内存 B+Tree。
///
/// 不接触 `ferrumdb-page::Page`（持久化是阶段 3 的事）。
///
/// - 内部节点 keys 严格升序；keys[i] 是 children[i] 与 children[i+1] 之间的分隔键
/// - 叶子节点 keys 严格升序；通过 `next` 形成单向链表
/// - 所有 key 必须可比较；所有 value 必须可 clone
pub struct BTree<K, V> {
    /// 根节点；空树为 `None`。
    root: Option<Box<Node<K, V>>>,
    /// 已插入的 (key, value) 总数（不区分重复 key 覆盖）。
    len: usize,
    /// PhantomData 维持与 Node 中 *mut 指针一致的 owned 语义。
    _marker: PhantomData<Box<Node<K, V>>>,
}

impl<K, V> Default for BTree<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> BTree<K, V> {
    /// 创建空 B+Tree。
    pub fn new() -> Self {
        Self {
            root: None,
            len: 0,
            _marker: PhantomData,
        }
    }

    /// 已插入条目数。
    pub fn len(&self) -> usize {
        self.len
    }

    /// 是否为空树。
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    /// 树高（叶子层算 1）；空树为 0。
    pub fn height(&self) -> usize {
        match &self.root {
            None => 0,
            Some(n) => Self::height_of(n),
        }
    }

    fn height_of(node: &Node<K, V>) -> usize {
        match node {
            Node::Leaf { .. } => 1,
            Node::Internal { children, .. } => 1 + Self::height_of(children[0].as_ref()),
        }
    }
}

impl<K: Ord + Clone, V: Clone> BTree<K, V> {
    /// 插入 (key, value)。重复 key 会被覆盖为新 value（不报错）。
    pub fn insert(&mut self, key: K, value: V) -> Result<(), BTreeError> {
        if self.root.is_none() {
            let mut leaf = Node::new_leaf();
            if let Node::Leaf { keys, values, .. } = leaf.as_mut() {
                keys.push(key);
                values.push(value);
            }
            self.root = Some(leaf);
            self.len = 1;
            return Ok(());
        }

        let mut root = self.root.take().expect("checked above");
        let (split_opt, inserted) = Self::insert_into(&mut root, key, value)?;
        match split_opt {
            Some(split) => {
                // Root split: build new internal root.
                let mut new_root = Node::new_internal();
                if let Node::Internal { keys, children } = new_root.as_mut() {
                    keys.push(split.key);
                    children.push(root);
                    children.push(split.right);
                }
                self.root = Some(new_root);
            }
            None => {
                self.root = Some(root);
            }
        }
        if inserted {
            self.len += 1;
        }
        Ok(())
    }

    /// 内部递归插入。
    /// 返回 `(split, inserted)`：
    /// - `split = Some(_)` 表示当前节点分裂了，需要父节点处理
    /// - `inserted = true` 表示新增了一条 (key, value)；`false` 表示覆盖了已有 key
    fn insert_into(
        node: &mut Node<K, V>,
        key: K,
        value: V,
    ) -> Result<(Option<Split<K, V>>, bool), BTreeError> {
        match node {
            Node::Leaf { keys, values, .. } => {
                let idx = crate::node::lower_bound(keys, &key);
                if idx < keys.len() && keys[idx] == key {
                    // Overwrite existing.
                    values[idx] = value;
                    return Ok((None, false));
                }
                keys.insert(idx, key);
                values.insert(idx, value);

                if keys.len() >= ORDER {
                    let (up_key, right_leaf) = Self::split_leaf(node)?;
                    return Ok((
                        Some(Split {
                            key: up_key,
                            right: right_leaf,
                        }),
                        true,
                    ));
                }
                Ok((None, true))
            }
            Node::Internal { keys, children } => {
                let idx = crate::node::lower_bound(keys, &key);
                // key[idx] separates children[idx] and children[idx+1]; if equal,
                // go right (separator key belongs to right subtree).
                let child_idx = if idx < keys.len() && keys[idx] == key {
                    idx + 1
                } else {
                    idx
                };
                let (split_opt, child_inserted) =
                    Self::insert_into(&mut children[child_idx], key, value)?;
                if let Some(split) = split_opt {
                    // Insert split.key at position idx, split.right as child at idx+1.
                    if idx < keys.len() && keys[idx] == split.key {
                        // Replace — should not happen with correct lower_bound logic.
                        keys[idx] = split.key;
                        children.insert(idx + 1, split.right);
                    } else {
                        keys.insert(idx, split.key);
                        children.insert(idx + 1, split.right);
                    }

                    if keys.len() >= ORDER {
                        let (up_key, right_internal) = Self::split_internal(node)?;
                        return Ok((
                            Some(Split {
                                key: up_key,
                                right: right_internal,
                            }),
                            child_inserted,
                        ));
                    }
                }
                Ok((None, child_inserted))
            }
        }
    }

    fn split_leaf(node: &mut Node<K, V>) -> Result<(K, Box<Node<K, V>>), BTreeError> {
        let (keys, values, next) = match node {
            Node::Leaf { keys, values, next, .. } => (keys, values, next),
            _ => return Err(BTreeError::InvalidNodeKind(0)),
        };
        let mid = keys.len() / 2;
        let right_keys = keys.split_off(mid);
        let right_values = values.split_off(mid);
        let up_key = right_keys[0].clone();

        // Old leaf's next becomes the new right's next; old leaf's next is the new right.
        let old_next = *next;
        let mut right_leaf = Node::new_leaf();
        if let Node::Leaf { keys: rk, values: rv, next: rn, .. } = right_leaf.as_mut() {
            *rk = right_keys;
            *rv = right_values;
            *rn = old_next;
        }
        *next = Some(right_leaf.as_mut() as *mut Node<K, V>);

        Ok((up_key, right_leaf))
    }

    fn split_internal(node: &mut Node<K, V>) -> Result<(K, Box<Node<K, V>>), BTreeError> {
        let (keys, children) = match node {
            Node::Internal { keys, children } => (keys, children),
            _ => return Err(BTreeError::InvalidNodeKind(1)),
        };
        let mid = keys.len() / 2;
        let up_key = keys.remove(mid);
        let right_keys = keys.split_off(mid);
        let right_children = children.split_off(mid + 1);

        let mut right_internal = Node::new_internal();
        if let Node::Internal { keys: rk, children: rc } = right_internal.as_mut() {
            *rk = right_keys;
            *rc = right_children;
        }
        Ok((up_key, right_internal))
    }

    /// 按主键查找。
    pub fn get(&self, key: &K) -> Result<Option<V>, BTreeError> {
        let mut cur = match self.root.as_ref() {
            None => return Ok(None),
            Some(n) => n.as_ref(),
        };
        loop {
            match cur {
                Node::Leaf { keys, values, .. } => {
                    let idx = crate::node::lower_bound(keys, key);
                    return Ok(if idx < keys.len() && &keys[idx] == key {
                        Some(values[idx].clone())
                    } else {
                        None
                    });
                }
                Node::Internal { keys, children } => {
                    let idx = crate::node::lower_bound(keys, key);
                    let child_idx = if idx < keys.len() && &keys[idx] == key {
                        idx + 1
                    } else {
                        idx
                    };
                    cur = children[child_idx].as_ref();
                }
            }
        }
    }

    /// 删除 key（v1 最小实现：找到则移除，节点下溢暂不修复）。
    /// 返回是否实际删除了某条记录。
    pub fn delete(&mut self, key: &K) -> Result<bool, BTreeError> {
        let root = match self.root.as_mut() {
            None => return Ok(false),
            Some(r) => r,
        };
        let removed = Self::delete_from(root, key)?;
        if removed {
            self.len -= 1;
            // If root is internal with single child, collapse.
            if let Node::Internal { keys, children } = root.as_ref() {
                if keys.is_empty() && children.len() == 1 {
                    let only = self.root.take().unwrap();
                    if let Node::Internal { children, .. } = *only {
                        self.root = Some(children.into_iter().next().unwrap());
                    }
                }
            }
            // If root is empty leaf, drop it.
            if let Some(Node::Leaf { keys, .. }) = self.root.as_deref() {
                if keys.is_empty() {
                    self.root = None;
                }
            }
        }
        Ok(removed)
    }

    fn delete_from(node: &mut Node<K, V>, key: &K) -> Result<bool, BTreeError> {
        match node {
            Node::Leaf { keys, values, .. } => {
                let idx = crate::node::lower_bound(keys, key);
                if idx < keys.len() && &keys[idx] == key {
                    keys.remove(idx);
                    values.remove(idx);
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Node::Internal { keys, children } => {
                let idx = crate::node::lower_bound(keys, key);
                let child_idx = if idx < keys.len() && &keys[idx] == key {
                    idx + 1
                } else {
                    idx
                };
                Self::delete_from(&mut children[child_idx], key)
            }
        }
    }

    /// 范围扫描：从 `start`（含）到 `end`（不含）。
    /// 返回借用迭代器，遍历叶子链表。
    pub fn scan_range<'a>(&'a self, start: &'a K, end: &'a K) -> ScanIter<'a, K, V> {
        ScanIter::new(self, start, end)
    }

    /// 全表扫描：从最小 key 到最大 key。
    pub fn scan_all<'a>(&'a self) -> ScanIter<'a, K, V> {
        // We can't easily construct a sentinel that compares as the smallest/largest
        // without changing the API. For v1, callers can use scan_range with manual bounds.
        // To keep the API ergonomic, we expose scan_via(&self, range_fn) instead.
        // Actually for ergonomics, let's just return the iter positioned at the first leaf.
        ScanIter::all(self)
    }
}

/// 借用迭代器，遍历叶子链表中落在 `[start, end)` 范围内的 (key, value)。
pub struct ScanIter<'a, K, V> {
    /// 当前叶子节点指针。
    cur_leaf: Option<*const Node<K, V>>,
    /// 当前叶子内索引。
    cur_idx: usize,
    /// 范围下界（含）；`None` 表示无穷小。
    start: Option<&'a K>,
    /// 范围上界（不含）；`None` 表示无穷大。
    end: Option<&'a K>,
    /// 是否已结束。
    done: bool,
    /// 借用标记。
    _marker: PhantomData<&'a Node<K, V>>,
}

impl<'a, K: Ord, V> ScanIter<'a, K, V> {
    fn new(tree: &'a BTree<K, V>, start: &'a K, end: &'a K) -> Self {
        let mut iter = Self {
            cur_leaf: None,
            cur_idx: 0,
            start: Some(start),
            end: Some(end),
            done: tree.root.is_none(),
            _marker: PhantomData,
        };
        iter.position_first_leaf(tree);
        iter
    }

    fn all(tree: &'a BTree<K, V>) -> Self {
        let mut iter = Self {
            cur_leaf: None,
            cur_idx: 0,
            start: None,
            end: None,
            done: tree.root.is_none(),
            _marker: PhantomData,
        };
        iter.position_first_leaf(tree);
        iter
    }

    fn position_first_leaf(&mut self, tree: &'a BTree<K, V>) {
        // Walk down to the leftmost leaf containing start (if any).
        let mut cur = match tree.root.as_ref() {
            None => {
                self.done = true;
                return;
            }
            Some(n) => n.as_ref(),
        };
        loop {
            match cur {
                Node::Leaf { keys, .. } => {
                    let start_key = self.start;
                    if let Some(s) = start_key {
                        let idx = crate::node::lower_bound(keys, s);
                        self.cur_idx = idx;
                    } else {
                        self.cur_idx = 0;
                    }
                    self.cur_leaf = Some(cur as *const Node<K, V>);
                    return;
                }
                Node::Internal { keys, children } => {
                    let start_key = self.start;
                    let idx = if let Some(s) = start_key {
                        crate::node::lower_bound(keys, s)
                    } else {
                        0
                    };
                    cur = children[idx].as_ref();
                }
            }
        }
    }
}

impl<'a, K: Ord, V> Iterator for ScanIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        loop {
            let leaf = self.cur_leaf?;
            // SAFETY: leaf pointer comes from tree.root which we borrow from 'a tree.
            // We only read keys/values through this pointer while the tree is alive.
            let leaf_ref: &Node<K, V> = unsafe { &*leaf };
            match leaf_ref {
                Node::Leaf { keys, values, next, .. } => {
                    if self.cur_idx < keys.len() {
                        let k = &keys[self.cur_idx];
                        if let Some(end) = self.end {
                            if k >= end {
                                self.done = true;
                                return None;
                            }
                        }
                        let v = &values[self.cur_idx];
                        self.cur_idx += 1;
                        return Some((k, v));
                    }
                    // Move to next leaf.
                    match next {
                        Some(p) => {
                            self.cur_leaf = Some(*p as *const Node<K, V>);
                            self.cur_idx = 0;
                            // Continue loop.
                        }
                        None => {
                            self.done = true;
                            return None;
                        }
                    }
                }
                _ => {
                    self.done = true;
                    return None;
                }
            }
        }
    }
}

// Make sure MIN_KEYS is used somewhere so the constant isn't a dead warning.
#[allow(dead_code)]
fn _min_keys_used() -> usize {
    MIN_KEYS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tree() {
        let t: BTree<i32, i32> = BTree::new();
        assert_eq!(t.len(), 0);
        assert_eq!(t.height(), 0);
        assert!(t.get(&5).unwrap().is_none());
    }

    #[test]
    fn insert_and_get_single() {
        let mut t = BTree::new();
        t.insert(5, "five".to_string()).unwrap();
        assert_eq!(t.get(&5).unwrap(), Some("five".to_string()));
        assert_eq!(t.get(&6).unwrap(), None);
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn insert_overwrites_existing_key() {
        let mut t = BTree::new();
        t.insert(1, "a".to_string()).unwrap();
        t.insert(1, "b".to_string()).unwrap();
        assert_eq!(t.get(&1).unwrap(), Some("b".to_string()));
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn root_split_increases_height() {
        let mut t = BTree::new();
        for i in 0..(ORDER * 2) {
            t.insert(i as i32, i).unwrap();
        }
        assert_eq!(t.len(), ORDER * 2);
        // After 2*ORDER inserts with no deletes, height should be at least 2.
        assert!(t.height() >= 2);
    }

    #[test]
    fn ten_thousand_random_inserts_then_scan_all_in_order() {
        // Deterministic pseudo-random to keep test reproducible.
        let mut state: u64 = 0xC0FFEE;
        let mut next = || -> i32 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (state as i32) ^ ((state >> 32) as i32)
        };
        let mut t = BTree::new();
        let mut inserted = std::collections::HashSet::new();
        for _ in 0..10_000 {
            let k = next();
            if inserted.insert(k) {
                t.insert(k, k).unwrap();
            }
        }
        let scanned: Vec<i32> = t.scan_all().map(|(k, _)| *k).collect();
        let mut expected: Vec<i32> = inserted.iter().copied().collect();
        expected.sort();
        assert_eq!(scanned, expected);
    }

    #[test]
    fn range_scan_excludes_end() {
        let mut t = BTree::new();
        for i in 0..10 {
            t.insert(i, i * 10).unwrap();
        }
        let r: Vec<i32> = t.scan_range(&3, &7).map(|(k, _)| *k).collect();
        assert_eq!(r, vec![3, 4, 5, 6]);
    }

    #[test]
    fn delete_removes_key() {
        let mut t = BTree::new();
        for i in 0..100 {
            t.insert(i, i).unwrap();
        }
        assert_eq!(t.len(), 100);
        assert!(t.delete(&50).unwrap());
        assert_eq!(t.len(), 99);
        assert!(t.get(&50).unwrap().is_none());
        // Other keys still present.
        assert_eq!(t.get(&49).unwrap(), Some(49));
        assert_eq!(t.get(&51).unwrap(), Some(51));
    }

    #[test]
    fn delete_missing_key_returns_false() {
        let mut t = BTree::new();
        t.insert(1, 10).unwrap();
        assert!(!t.delete(&999).unwrap());
        assert_eq!(t.len(), 1);
    }
}
