//! Собирает содержимое папки `site` в статический массив на этапе сборки.
//!
//! Офлайн-сайт должен попасть в бинарник: при установке через `cargo install`
//! рядом с exe нет ни исходников, ни папки `site`. Список файлов генерируется
//! явно, чтобы пропущенный ресурс ломал сборку, а не приложение.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Обязательные файлы сайта: без них офлайн-версия не запустится.
const REQUIRED: [&str; 6] = [
    "index.html",
    "app.js",
    "style.css",
    "sw.js",
    "data/dictionary.tsv",
    "data/grammar.json",
];

fn main() {
    let site = Path::new(env!("CARGO_MANIFEST_DIR")).join("site");
    println!("cargo:rerun-if-changed={}", site.display());

    let mut files = Vec::new();
    collect(&site, &site, &mut files);
    files.sort();

    for name in REQUIRED {
        assert!(
            files.iter().any(|file| file == name),
            "в папке site нет {name}"
        );
    }

    let mut generated = String::from("// Сгенерировано build.rs: не редактировать вручную.\n");
    generated.push_str("pub(crate) const SITE_FILES: &[(&str, &[u8])] = &[\n");
    for relative in &files {
        let absolute = site.join(relative);
        writeln!(
            generated,
            "    ({relative:?}, include_bytes!({absolute:?})),"
        )
        .expect("форматирование списка файлов");
    }
    generated.push_str("];\n");

    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR")).join("site_files.rs");
    std::fs::write(out, generated).expect("запись сгенерированного файла");
}

/// Рекурсивно собирает относительные пути файлов.
fn collect(root: &Path, dir: &Path, files: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, files);
        } else if let Ok(relative) = path.strip_prefix(root) {
            files.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}
