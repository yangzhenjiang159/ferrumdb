//! 简单 CRUD 演示：用 FerrumDB M1 栈做增删改查。
//!
//! 从 ferrumdb-btree 目录运行：`cargo run --example crud`

use std::path::PathBuf;

use ferrumdb_btree::PersistentBtree;
use ferrumdb_page::{decode_row, encode_row, ColumnType, Row, Schema, Value};
use ferrumdb_space::Space;

fn schema() -> Schema {
    Schema {
        columns: vec!["id".into(), "name".into(), "age".into()],
        types: vec![ColumnType::I64, ColumnType::Bytes, ColumnType::I64],
        primary_key: Some(0),
    }
}

fn encode_user(id: i32, name: &str, age: i32) -> Vec<u8> {
    let row = Row {
        values: vec![
            Value::I64(id as i64),
            Value::Bytes(name.as_bytes().to_vec()),
            Value::I64(age as i64),
        ],
    };
    encode_row(&row, &schema()).expect("encode_row")
}

fn decode_user(bytes: &[u8]) -> (i32, String, i32) {
    let row = decode_row(bytes, &schema()).expect("decode_row");
    let id = match &row.values[0] {
        Value::I64(n) => *n as i32,
        _ => panic!("expected I64 for id"),
    };
    let name = match &row.values[1] {
        Value::Bytes(b) => String::from_utf8(b.clone()).expect("valid utf8"),
        _ => panic!("expected Bytes for name"),
    };
    let age = match &row.values[2] {
        Value::I64(n) => *n as i32,
        _ => panic!("expected I64 for age"),
    };
    (id, name, age)
}

fn key(id: i32) -> Vec<u8> {
    id.to_be_bytes().to_vec()
}

struct Db {
    space: Space,
    tree: PersistentBtree,
}

impl Db {
    fn open(path: PathBuf) -> Self {
        let need_create = !path.exists();
        let mut space = if need_create {
            Space::create(&path).expect("create space")
        } else {
            Space::open(&path).expect("open space")
        };
        let tree = if need_create {
            let t = PersistentBtree::create(&mut space).expect("create tree");
            space
                .set_root_page_id(t.root_page_id())
                .expect("set root");
            t
        } else {
            let root = space.superblock().root_page_id.expect("root_page_id");
            PersistentBtree::open(&mut space, root).expect("open tree")
        };
        Self { space, tree }
    }

    fn insert(&mut self, id: i32, name: &str, age: i32) {
        let k = key(id);
        let v = encode_user(id, name, age);
        self.tree.insert(&mut self.space, k, v).expect("insert");
    }

    fn get(&mut self, id: i32) -> Option<(i32, String, i32)> {
        self.tree
            .get(&mut self.space, &key(id))
            .expect("get")
            .map(|v| decode_user(&v))
    }

    fn update(&mut self, id: i32, name: &str, age: i32) -> bool {
        if self.get(id).is_none() {
            return false;
        }
        self.insert(id, name, age);
        true
    }

    fn delete(&mut self, id: i32) -> bool {
        self.tree.delete(&mut self.space, &key(id)).expect("delete")
    }

    fn list(&mut self) -> Vec<(i32, String, i32)> {
        // 全字节范围覆盖所有 i32 (包括负数)。注意 i32::MIN BE = [0x80,0,0,0]
        // 比 id=1 ([0,0,0,1]) 还大, 不能直接当 start。
        let start = vec![0u8; 4];
        let end = vec![0xFFu8; 4];
        self.tree
            .scan_range(&mut self.space, &start, &end)
            .expect("scan")
            .into_iter()
            .map(|(_k, v)| decode_user(&v))
            .collect()
    }

    fn count(&mut self) -> usize {
        self.list().len()
    }
}

fn print_user(u: &(i32, String, i32)) {
    println!("  id={}, name=\"{}\", age={}", u.0, u.1, u.2);
}

fn main() {
    let path: PathBuf = std::env::temp_dir().join("ferrumdb_crud_demo.ibd");
    let _ = std::fs::remove_file(&path);
    println!("== FerrumDB Simple CRUD Demo ==");
    println!("tablespace: {}", path.display());
    println!();

    let mut db = Db::open(path.clone());

    println!("[INSERT] 3 rows");
    db.insert(1, "Alice", 30);
    db.insert(2, "Bob", 25);
    db.insert(3, "Charlie", 35);
    println!("  count = {}", db.count());
    println!();

    println!("[GET id=2]");
    match db.get(2) {
        Some(u) => print_user(&u),
        None => println!("  not found"),
    }
    println!();

    println!("[GET id=999] (not found expected)");
    match db.get(999) {
        Some(u) => print_user(&u),
        None => println!("  not found (expected)"),
    }
    println!();

    println!("[UPDATE id=2 -> name=Bobby age=26]");
    if db.update(2, "Bobby", 26) {
        println!("  updated");
    } else {
        println!("  not found");
    }
    if let Some(u) = db.get(2) {
        print_user(&u);
    }
    println!();

    println!("[LIST all]");
    for u in db.list() {
        print_user(&u);
    }
    println!();

    println!("[DELETE id=1]");
    if db.delete(1) {
        println!("  deleted");
    } else {
        println!("  not found");
    }
    println!("  count after delete = {}", db.count());
    println!();

    println!("[LIST after delete]");
    for u in db.list() {
        print_user(&u);
    }

    drop(db);
    let _ = std::fs::remove_file(&path);
    println!("\n[cleanup] removed {}", path.display());
}
