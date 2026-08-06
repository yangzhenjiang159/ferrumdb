//! 阶段 6 集成测试：二级索引与回表（验收 AC1–AC7）。
//!
//! 覆盖：
//! - AC1 二级索引点查 / 不存在返回 None
//! - AC2 二级索引范围扫描 + 回表，结果按索引键有序
//! - AC3 同一 pk 多二级索引一致
//! - AC4 存储层持久化（记录 root id 重开树）
//! - AC6 唯一索引冲突返回 DuplicateKey 且无部分写入
//! - AC7 非唯一索引多个 pk 共享同一索引 key

use crate::{EngineError, FerrumEngine, IndexMeta, RangeBound, StorageEngine};
use ferrumdb_page::{ColumnType, Row, Schema, Value};
use tempfile::TempDir;

/// `users(id PK, name Bytes)`，主键列下标 0。
fn user_schema() -> Schema {
    Schema {
        columns: vec!["id".into(), "name".into()],
        types: vec![ColumnType::I64, ColumnType::Bytes],
        primary_key: Some(0),
    }
}

fn row(id: i64, name: &str) -> Row {
    Row {
        values: vec![Value::I64(id), Value::Bytes(name.as_bytes().to_vec())],
    }
}

fn tmp_engine() -> (TempDir, FerrumEngine) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.ibd");
    let engine = FerrumEngine::open_or_create(&path).expect("open_or_create");
    (dir, engine)
}

fn set_up(name_idx: &str, is_unique: bool) -> (TempDir, FerrumEngine) {
    let (dir, mut engine) = tmp_engine();
    engine.create_table("users", user_schema()).unwrap();
    engine
        .create_index(
            "users",
            IndexMeta {
                name: name_idx.into(),
                columns: vec![1], // name
                is_unique,
            },
        )
        .unwrap();
    (dir, engine)
}

/// AC1：二级索引点查正确；不存在的 key 返回 None。
#[test]
fn get_by_index_point_lookup() {
    let (_dir, mut engine) = set_up("idx_name", false);
    engine.insert("users", row(1, "alice")).unwrap();
    engine.insert("users", row(2, "bob")).unwrap();

    let got = engine
        .get_by_index("users", "idx_name", Value::Bytes(b"alice".to_vec()))
        .unwrap()
        .expect("row for alice");
    assert_eq!(got, row(1, "alice"));

    let missing = engine
        .get_by_index("users", "idx_name", Value::Bytes(b"carol".to_vec()))
        .unwrap();
    assert_eq!(missing, None);
}

/// AC2：二级索引范围扫描 + 回表，结果按索引键有序，且回表得到完整行。
#[test]
fn scan_index_range_with_lookup() {
    let (_dir, mut engine) = set_up("idx_name", false);
    for (id, name) in [(3, "charlie"), (1, "alice"), (2, "bob")] {
        engine.insert("users", row(id, name)).unwrap();
    }
    let range = RangeBound {
        start: Some(Value::Bytes(b"a".to_vec())),
        end: Some(Value::Bytes(b"c".to_vec())), // 半开 [a, c)
    };
    let rows: Vec<Row> = engine
        .scan_index("users", "idx_name", range)
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    // 按 name 有序：alice, bob；charlie 的 name 以 'c' 开头，不在 [a,c)。
    assert_eq!(rows, vec![row(1, "alice"), row(2, "bob")]);
}

/// AC3：同一 pk 多个二级索引，insert 后所有索引都能查到该 pk。
#[test]
fn multiple_indexes_consistent_for_same_pk() {
    let (_dir, mut engine) = tmp_engine();
    engine.create_table("users", user_schema()).unwrap();
    engine
        .create_index(
            "users",
            IndexMeta {
                name: "idx_name".into(),
                columns: vec![1],
                is_unique: false,
            },
        )
        .unwrap();
    // 再建一个覆盖 id+name 的复合索引（验证复合列下标）。
    engine
        .create_index(
            "users",
            IndexMeta {
                name: "idx_id_name".into(),
                columns: vec![0, 1],
                is_unique: false,
            },
        )
        .unwrap();
    engine.insert("users", row(7, "dave")).unwrap();

    let via_name = engine
        .get_by_index("users", "idx_name", Value::Bytes(b"dave".to_vec()))
        .unwrap()
        .expect("row via idx_name");
    assert_eq!(via_name, row(7, "dave"));

    // 复合索引：get_by_index 只匹配首列（id=7），也应回表到同一行。
    let via_composite = engine
        .get_by_index("users", "idx_id_name", Value::I64(7))
        .unwrap()
        .expect("row via idx_id_name");
    assert_eq!(via_composite, row(7, "dave"));
}

/// AC4：存储层持久化——记录 root id，重开 Space + PersistentBtree 后点查/扫描正确。
#[test]
fn storage_persistence_reopen() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.ibd");
    let (clustered_root, index_roots);
    {
        let mut engine = FerrumEngine::open_or_create(&path).unwrap();
        engine.create_table("users", user_schema()).unwrap();
        engine
            .create_index(
                "users",
                IndexMeta {
                    name: "idx_name".into(),
                    columns: vec![1],
                    is_unique: false,
                },
            )
            .unwrap();
        for i in 0..50 {
            engine
                .insert("users", row(i, &format!("user{i:03}")))
                .unwrap();
        }
        (clustered_root, index_roots) = engine.root_ids("users").unwrap();
        // 50 条 > ORDER(64)? 不触发分裂；但 root id 记录本身应正确。
    }
    let idx_root = index_roots
        .iter()
        .find(|(name, _)| name == "idx_name")
        .expect("idx_name root")
        .1;

    // 重新打开 Space，用记录的 root id 打开聚簇与二级树。
    let mut space = ferrumdb_space::Space::open(&path).unwrap();
    let clustered = ferrumdb_btree::PersistentBtree::open(&mut space, clustered_root).unwrap();
    // 聚簇点查：用 engine 同款编码（encode_key(I64)）。
    let pk_bytes = ferrumdb_page::encode_key(&Value::I64(5));
    let row_bytes = clustered.get(&mut space, &pk_bytes).unwrap().expect("row 5");
    let decoded = ferrumdb_page::decode_row(&row_bytes, &user_schema()).unwrap();
    assert_eq!(decoded, row(5, "user005"));

    let secondary = ferrumdb_btree::PersistentBtree::open(&mut space, idx_root).unwrap();
    // 二级扫描：前缀为 "user005" 编码，范围 [P, P∥upper_bound(I64))。
    let p = ferrumdb_page::encode_index_key(&[Value::Bytes(b"user005".to_vec())]);
    let end = [p.clone(), ferrumdb_page::upper_bound(ColumnType::I64)].concat();
    let hits = secondary.scan_range(&mut space, &p, &end).unwrap();
    assert_eq!(hits.len(), 1);
    let (_, stored_pk) = &hits[0];
    let (pk_val, _) = ferrumdb_page::decode_key(stored_pk, ColumnType::I64).unwrap();
    assert_eq!(pk_val, Value::I64(5));
}

/// AC6：唯一索引冲突返回 DuplicateKey，且冲突后无部分写入。
#[test]
fn unique_index_conflict_no_partial_write() {
    let (_dir, mut engine) = set_up("uniq_name", true);
    engine.insert("users", row(1, "alice")).unwrap();

    // 用已存在 name 插入新 pk → 冲突。
    let err = engine.insert("users", row(2, "alice")).unwrap_err();
    assert!(matches!(err, EngineError::DuplicateKey), "got {err:?}");

    // 冲突后：pk=2 未写入聚簇，二级索引仍指向 alice(pk=1)。
    assert_eq!(engine.get_by_pk("users", Value::I64(2)).unwrap(), None);
    let via_index = engine
        .get_by_index("users", "uniq_name", Value::Bytes(b"alice".to_vec()))
        .unwrap()
        .expect("alice still there");
    assert_eq!(via_index, row(1, "alice"));

    // 冲突后重新插入一个不同 name 的 pk=2 应成功。
    engine.insert("users", row(2, "bob")).unwrap();
    assert_eq!(
        engine.get_by_pk("users", Value::I64(2)).unwrap(),
        Some(row(2, "bob"))
    );
}

/// AC6：主键冲突也返回 DuplicateKey。
#[test]
fn primary_key_conflict_duplicate() {
    let (_dir, mut engine) = set_up("idx_name", false);
    engine.insert("users", row(1, "alice")).unwrap();
    let err = engine.insert("users", row(1, "bob")).unwrap_err();
    assert!(matches!(err, EngineError::DuplicateKey), "got {err:?}");
    // 原行未被覆盖。
    assert_eq!(
        engine.get_by_pk("users", Value::I64(1)).unwrap(),
        Some(row(1, "alice"))
    );
}

/// AC7：非唯一索引，多个不同 pk 可共享同一索引 key，scan_range 按 (index_key, pk) 有序返回。
#[test]
fn non_unique_index_shared_key_ordered() {
    let (_dir, mut engine) = set_up("idx_name", false);
    // 三个不同 pk 共享 name "dup"。
    engine.insert("users", row(5, "dup")).unwrap();
    engine.insert("users", row(2, "dup")).unwrap();
    engine.insert("users", row(9, "dup")).unwrap();

    let rows: Vec<Row> = engine
        .scan_index("users", "idx_name", RangeBound::full())
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    // 完整索引扫描：按 (name, pk) 有序。pk 2,5,9 均共享 "dup"。
    let expected = vec![row(2, "dup"), row(5, "dup"), row(9, "dup")];
    assert_eq!(rows, expected);

    // get_by_index 非唯一返回最小 pk。
    let first = engine
        .get_by_index("users", "idx_name", Value::Bytes(b"dup".to_vec()))
        .unwrap()
        .expect("first dup row");
    assert_eq!(first, row(2, "dup"));
}

/// AC1 补充：get_by_pk 只走聚簇，与二级索引结果一致。
#[test]
fn get_by_pk_uses_clustered_only() {
    let (_dir, mut engine) = set_up("idx_name", false);
    engine.insert("users", row(42, "eve")).unwrap();
    let got = engine.get_by_pk("users", Value::I64(42)).unwrap().expect("row 42");
    assert_eq!(got, row(42, "eve"));
    // 主键不存在返回 None。
    assert_eq!(engine.get_by_pk("users", Value::I64(999)).unwrap(), None);
}

/// 范围扫描聚簇（scan）按主键有序。
#[test]
fn clustered_scan_ordered_by_pk() {
    let (_dir, mut engine) = set_up("idx_name", false);
    for id in [3, 1, 2] {
        engine.insert("users", row(id, "n")).unwrap();
    }
    let rows: Vec<Row> = engine
        .scan("users", RangeBound::full())
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows, vec![row(1, "n"), row(2, "n"), row(3, "n")]);

    // 主键范围 [2, 3)。
    let range = RangeBound {
        start: Some(Value::I64(2)),
        end: Some(Value::I64(3)),
    };
    let rows: Vec<Row> = engine
        .scan("users", range)
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows, vec![row(2, "n")]);
}

/// 阶段 6 范围之外的方法返回 Unsupported。
#[test]
fn unsupported_methods_return_unsupported() {
    let (_dir, mut engine) = tmp_engine();
    engine.create_table("users", user_schema()).unwrap();
    assert!(matches!(
        engine.update("users", Value::I64(1), row(1, "x")),
        Err(EngineError::Unsupported(_))
    ));
    assert!(matches!(
        engine.delete("users", Value::I64(1)),
        Err(EngineError::Unsupported(_))
    ));
    assert!(matches!(
        engine.drop_table("users"),
        Err(EngineError::Unsupported(_))
    ));
    assert!(matches!(engine.begin(), Err(EngineError::Unsupported(_))));
}
