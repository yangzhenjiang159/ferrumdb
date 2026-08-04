# Design — 阶段 3 持久化 B+Tree + 表空间

## 1. 模块依赖

```
ferrumdb-page  ← ferrumdb-space  ← ferrumdb-btree (PersistentBTree)
```

新增依赖：`tempfile`（workspace dep，仅 dev-dependency）用于集成测试。

## 2. ferrumdb-space 详细设计

### 2.1 文件布局

```
tablespace.ibd
├── Page 0       Superblock  (PageType::Superblock)
├── Page 1       Reserved    (PageType::Free, future catalog)
├── Page 2       Free list head or first B+Tree node
├── Page 3..N    ...
```

文件长度 = `n * PAGE_SIZE`，通过 `std::fs::File::set_len` 扩展。

### 2.2 Superblock（page 0 的 user_data）

Superblock **不是** page header；它是 page 0 的 user_data 里的内容。Page 自身的 32-byte header + 8-byte footer + CRC32 仍然保护整个 16KB 块。

```rust
pub struct Superblock {
    pub magic: u32,              // = PAGE_MAGIC = 0xFEDB_0001
    pub version: u32,            // 当前 1
    pub page_size: u32,          // = PAGE_SIZE = 16384
    pub free_list_head: Option<PageId>,
    pub root_page_id: Option<PageId>,  // 阶段 3 借用这一项存放 B+Tree root
    pub last_lsn: u64,           // 阶段 5 WAL 用，v1 写 0
}
```

序列化：little-endian 顺序写：

```
[magic:u32][version:u32][page_size:u32]
[free_list_head_is_some:u8][free_list_head:u32]
[root_page_id_is_some:u8][root_page_id:u32]
[last_lsn:u64]
```

固定 26 字节；其余 user_data 字节写 0。

### 2.3 Space 结构

```rust
pub struct Space {
    file: File,
    path: PathBuf,
    superblock: Superblock,
    dirty: bool,  // superblock / free-list 修改后置 true
}
```

### 2.4 方法

```rust
impl Space {
    pub fn create(path: impl AsRef<Path>) -> Result<Self, SpaceError>;
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SpaceError>;
    pub fn close(self) -> Result<(), SpaceError>;  // consumes self, syncs + drops

    pub fn read_page(&mut self, page_id: PageId) -> Result<Page, SpaceError>;
    pub fn write_page(&mut self, page_id: PageId, page: &Page) -> Result<(), SpaceError>;

    pub fn allocate_page(&mut self) -> Result<PageId, SpaceError>;
    pub fn free_page(&mut self, page_id: PageId) -> Result<(), SpaceError>;

    pub fn superblock(&self) -> &Superblock;
    pub fn set_root_page_id(&mut self, id: PageId) -> Result<(), SpaceError>;
    pub fn sync_all(&mut self) -> Result<(), SpaceError>;
}
```

### 2.5 PageId ↔ offset

```rust
fn offset_of(page_id: PageId) -> u64 {
    page_id as u64 * PAGE_SIZE as u64
}
```

**唯一**计算点，在 `read_page` / `write_page` / `set_len` 中调用。

### 2.6 Free list

Free list 是 `PageType::Free` 页的单向链表。每个 free 页的 user_data：

```
[next_is_some:u8][next_page_id:u32]    // 5 bytes
```

- 头插：`free_page(id)` → 把 `[is_some=old_head_is_some, old_head?]` 写入该页 → 更新 superblock.free_list_head = Some(id)
- 取：`allocate_page` 优先 `pop_head()`，否则 extend

### 2.7 Fsync 策略

- `sync_all` 在以下操作后调用：
  - `write_page`（写用户数据，立即 fsync）
  - `allocate_page` / `free_page` 修改 free list 后
  - `set_root_page_id` 后
- 集成测试用 `tempfile::tempdir()`，不需要手动 fsync 优化（v1 直接 sync_all）

## 3. ferrumdb-btree 持久化设计

### 3.1 PageSource trait

PersistentBTree 通过 trait 抽象底层存储，方便测试用 mock：

```rust
pub trait PageSource {
    fn read_page(&mut self, page_id: PageId) -> Result<Page, SpaceError>;
    fn write_page(&mut self, page_id: PageId, page: &Page) -> Result<(), SpaceError>;
    fn allocate_page(&mut self) -> Result<PageId, SpaceError>;
}
```

`Space` 直接 `impl PageSource`（forward 方法）。测试用 `MockPageSource`。

### 3.2 节点 ↔ Page 序列化

每个节点占用一个 Page，user_data 布局：

```
[kind: u8]               // 0=Internal, 1=Leaf
[key_count: u16 LE]
[is_root: u8]            // 0/1（v1 简化：root 通过 Space.superblock.root_page_id 跟踪）
[padding: 4 bytes]       // 对齐
[key_0 bytes][key_1 bytes]...[key_(n-1) bytes]
[child_0: u32 LE][child_1: u32 LE]...[child_n: u32 LE]  // 仅 Internal
[value_0 bytes][value_1 bytes]...[value_(n-1) bytes]    // 仅 Leaf
[next_leaf_page_id: u32 LE]                            // 仅 Leaf (u32::MAX = None)
```

变长字段用 `[len:u32 LE][bytes]` 编码（与 Row 编码风格一致）。

### 3.3 PersistentBTree 结构

```rust
pub struct PersistentBtree<K, V> {
    source: Box<dyn PageSource>,
    root: PageId,
    height: usize,
    len: usize,
}
```

注意：`Box<dyn PageSource>` 让 trait object 安全（仅方法 + `&mut self`）。

### 3.4 插入流程

```text
fn insert(&mut self, key, value):
    let root_page = self.source.read_page(self.root)?;
    let split_opt = insert_into_page(root_page, key, value, &mut self.source)?;
    if let Some((up_key, new_right_page_id)) = split_opt:
        // Root split: allocate new page for new root
        let new_root_id = self.source.allocate_page()?;
        let mut new_root = Page::new(new_root_id, PageType::Index);
        // serialize Internal { keys: [up_key], children: [self.root, new_right_page_id] }
        self.source.write_page(new_root_id, &new_root)?;
        self.root = new_root_id;
        self.height += 1;
        // update superblock.root_page_id
```

### 3.5 集成测试策略

```rust
#[test]
fn persistent_1000_keys_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.ibd");

    // Phase A: create + insert
    {
        let mut space = Space::create(&path).unwrap();
        let mut tree = PersistentBtree::<i32, i32>::create(&mut space).unwrap();
        for i in 0..1000 { tree.insert(i, i * 10).unwrap(); }
        tree.flush(&mut space).unwrap();
        space.set_root_page_id(tree.root()).unwrap();
    }

    // Phase B: reopen + verify
    {
        let mut space = Space::open(&path).unwrap();
        let root_id = space.superblock().root_page_id.unwrap();
        let tree = PersistentBtree::<i32, i32>::open(&mut space, root_id).unwrap();
        for i in 0..1000 {
            assert_eq!(tree.get(&i).unwrap(), Some(i * 10));
        }
    }
}
```

## 4. 错误类型汇总

### 4.1 SpaceError (新增)

```rust
#[derive(Debug, thiserror::Error)]
pub enum SpaceError {
    #[error("space io: {0}")]
    Io(#[from] std::io::Error),

    #[error("page id out of range: {0}")]
    PageIdOutOfRange(PageId),

    #[error("free list corrupted at page {0}")]
    FreeListCorrupted(PageId),

    #[error("superblock invalid magic")]
    SuperblockInvalidMagic,

    #[error("superblock page size mismatch: file {file}, build {build}")]
    SuperblockPageSizeMismatch { file: u32, build: u32 },

    #[error("superblock version {0} not supported")]
    SuperblockVersionUnsupported(u32),

    #[error("not initialized")]
    NotInitialized,
}
```

### 4.2 BTreeError（新增变体）

```rust
#[error("space error: {0}")]
Space(#[from] SpaceError),

#[error("invalid node page (kind byte {0})")]
InvalidNodeKind(u8),
```

## 5. 文件改动清单

```
crates/ferrumdb-space/src/
├── lib.rs              (重导出 Space, Superblock, PageId, SpaceError)
├── error.rs            (新增: SpaceError)
├── space.rs            (新增: Space + open/create/read/write)
├── superblock.rs       (新增: Superblock + serialize/deserialize)
├── free_list.rs        (新增: Free list push/pop helpers)
└── page_source.rs      (新增: PageSource trait + Space impl)

crates/ferrumdb-btree/src/
├── lib.rs              (重导出 PersistentBtree)
├── tree.rs             (+ PersistentBtree impl)
└── persist.rs          (新增: 节点 ↔ Page 序列化)

Cargo.toml (root)
└── [workspace.dependencies] + tempfile = "3"

crates/ferrumdb-space/Cargo.toml
└── [dev-dependencies] + tempfile = { workspace = true }
```

## 6. 风险与回滚

- **风险 A**：节点序列化格式错误导致 reopen 后无法解析 → 通过 round-trip 测试 + corruption 测试覆盖
- **风险 B**：fsync 路径出错导致数据丢失 → 每个 metadata 写后立即 sync_all 测试
- **回滚**：节点页是普通 Page，重启时读不出来就返回 `InvalidNodeKind`；不会 crash 整个进程
