//! LRU 顺序跟踪。

use crate::frame::FrameId;

/// LRU 顺序容器（front = MRU, back = LRU）。
///
/// Vec 实现，O(n) 移动；capacity 通常 < 10k，移动代价可接受。
#[derive(Debug, Default, Clone)]
pub struct LruOrder {
    order: Vec<FrameId>,
}

impl LruOrder {
    pub fn new() -> Self {
        Self { order: Vec::new() }
    }

    /// 把 frame_id 标记为最近使用。
    pub fn touch(&mut self, frame_id: FrameId) {
        if let Some(pos) = self.order.iter().position(|&id| id == frame_id) {
            self.order.remove(pos);
        }
        self.order.insert(0, frame_id);
    }

    /// 移除 frame_id（淘汰时调用）。
    pub fn remove(&mut self, frame_id: FrameId) {
        self.order.retain(|&id| id != frame_id);
    }

    /// 返回所有 frame_id（MRU → LRU 顺序），用于遍历淘汰候选。
    pub fn iter_lru(&self) -> impl Iterator<Item = FrameId> + '_ {
        self.order.iter().rev().copied()
    }


}
