//! Proves the app can start from nothing: HOME is pointed at an empty directory, then every
//! step of a first launch runs in order.
//!
//! This exists because of a real bug: `db_path()` built a path without creating its
//! directory, so SQLite failed to open the file and the app died before a single window
//! appeared — on a genuinely clean install only.

use auto_transcript_lib::paths;
use auto_transcript_lib::settings::Settings;
use auto_transcript_lib::store::db::Db;

fn main() {
    let tmp = std::env::temp_dir().join(format!("at-freshstart-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("temp home");
    // SAFETY: dijalankan di awal main, sebelum thread lain dibuat.
    unsafe { std::env::set_var("HOME", &tmp) };
    println!("Temporary HOME: {}", tmp.display());

    let dir = tmp.join("Library/Application Support/auto-transcript");
    assert!(!dir.exists(), "the data directory should not exist yet");

    let mut fail = 0;
    let mut check = |label: &str, result: Result<(), String>| match result {
        Ok(()) => println!("  ok     {label}"),
        Err(e) => {
            println!("  FAILED {label}: {e}");
            fail += 1;
        }
    };

    check(
        "open the database",
        Db::open().map(|_| ()).map_err(|e| e.to_string()),
    );
    check(
        "load settings",
        Ok(()).map(|_: ()| {
            let _ = Settings::load();
        }),
    );
    check(
        "save settings",
        Settings::default().save().map_err(|e| e.to_string()),
    );
    check(
        "models directory",
        paths::models_dir().map(|_| ()).map_err(|e| e.to_string()),
    );
    check(
        "recordings directory",
        paths::recordings_dir().map(|_| ()).map_err(|e| e.to_string()),
    );
    check(
        "logs directory",
        paths::log_dir().map(|_| ()).map_err(|e| e.to_string()),
    );

    println!("\nFiles created:");
    for entry in walk(&tmp) {
        println!("  {}", entry.strip_prefix(&tmp).unwrap().display());
    }

    let _ = std::fs::remove_dir_all(&tmp);
    if fail > 0 {
        eprintln!("\n{fail} step(s) failed");
        std::process::exit(1);
    }
    println!("\nPASSED: a clean first start works.");
}

fn walk(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(p) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&p) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}
