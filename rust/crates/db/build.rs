use std::{fs, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=migrations");
    emit_migration_files(Path::new("migrations"));
}

fn emit_migration_files(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            emit_migration_files(&path);
        } else {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}
