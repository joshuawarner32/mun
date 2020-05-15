use crate::db::IrDatabase;
use hir::mock::MockDatabase;
use hir::{FileId, RelativePathBuf, SourceDatabase, SourceRoot, SourceRootId};
use std::sync::Arc;

pub fn single_file_mock_db(text: &str) -> (IrDatabase<MockDatabase>, FileId) {
    let mut db: MockDatabase = Default::default();

    let mut source_root = SourceRoot::default();
    let source_root_id = SourceRootId(0);

    let text = Arc::new(text.to_owned());
    let rel_path = RelativePathBuf::from("main.mun");
    let file_id = FileId(0);
    db.set_file_relative_path(file_id, rel_path.clone());
    db.set_file_text(file_id, Arc::new(text.to_string()));
    db.set_file_source_root(file_id, source_root_id);
    source_root.insert_file(rel_path, file_id);

    db.set_source_root(source_root_id, Arc::new(source_root));
    (IrDatabase::new(db), file_id)
}

pub fn log(db: &IrDatabase<MockDatabase>, f: impl FnOnce()) -> Vec<salsa::Event<MockDatabase>> {
    *db.hir_db().events.lock() = Some(Vec::new());
    f();
    db.hir_db().events.lock().take().unwrap()
}

pub fn log_executed(db: &IrDatabase<MockDatabase>, f: impl FnOnce()) -> Vec<String> {
    let events = log(db, f);
    events
        .into_iter()
        .filter_map(|e| match e.kind {
            // This pretty horrible, but `Debug` is the only way to inspect
            // QueryDescriptor at the moment.
            salsa::EventKind::WillExecute { database_key } => {
                Some(format!("{:?}", database_key))
            }
            _ => None,
        })
        .collect()
}
