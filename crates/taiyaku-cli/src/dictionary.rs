//! `taiyaku dictionary import`: build a language's dictionary for the book
//! reader's lookups from kaikki.org's JSON lines.

use std::path::{Path, PathBuf};

/// `dictionary import <lang> <kaikki.jsonl> --dir DIR`, writing `DIR/<lang>.db`.
pub fn command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("import") => {
            let mut dir = None;
            let mut positional = Vec::new();
            let mut rest = args[1..].iter();
            while let Some(arg) = rest.next() {
                if arg == "--dir" {
                    dir = rest.next();
                } else {
                    positional.push(arg);
                }
            }
            let dir = dir.ok_or("dictionary import needs --dir DIR")?;
            let [lang, source] = positional.as_slice() else {
                return Err(
                    "usage: taiyaku dictionary import <lang> <kaikki.jsonl> --dir DIR".into(),
                );
            };
            let (path, done) = import(lang, Path::new(source), Path::new(dir))?;
            println!(
                "wrote {}: {} entries, {} forms, {} lines passed over",
                path.display(),
                done.entries,
                done.forms,
                done.skipped
            );
            Ok(())
        }
        _ => Err("usage: taiyaku dictionary import <lang> <kaikki.jsonl> --dir DIR".into()),
    }
}

/// Build `dir/<lang>.db` from `source`, beside it first and moved into place
/// when whole, so a server never opens one half written.
fn import(
    lang: &str,
    source: &Path,
    dir: &Path,
) -> Result<(PathBuf, taiyaku_library::dictionary::Imported), String> {
    // The name is a file's, so only a language code is one.
    if !(2..=3).contains(&lang.len()) || !lang.bytes().all(|b| b.is_ascii_lowercase()) {
        return Err(format!("`{lang}` is no language code, such as es or ru"));
    }
    let file = std::fs::File::open(source).map_err(|e| format!("{}: {e}", source.display()))?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(format!("{lang}.db"));
    let partial = dir.join(format!("{lang}.db.part"));
    let _ = std::fs::remove_file(&partial);
    let conn =
        rusqlite::Connection::open(&partial).map_err(|e| format!("{}: {e}", partial.display()))?;
    let done = taiyaku_library::dictionary::import(&conn, std::io::BufReader::new(file))?;
    drop(conn);
    std::fs::rename(&partial, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((path, done))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dictionary_is_built_whole_under_its_language_and_looks_up() {
        let tmp = taiyaku_read::test_support::TempDir::new("dictionary-import");
        let dir = tmp.path().to_path_buf();
        let source = dir.join("es.jsonl");
        std::fs::write(
            &source,
            r#"{"word":"hablar","pos":"verb","senses":[{"glosses":["to speak"]}],"forms":[{"form":"hablaron","tags":["plural"]}]}"#,
        )
        .unwrap();
        let (path, done) = import("es", &source, &dir.join("out")).expect("imports");
        assert_eq!(path, dir.join("out/es.db"));
        assert_eq!((done.entries, done.forms), (1, 1));
        assert!(!dir.join("out/es.db.part").exists());
        let conn = rusqlite::Connection::open(&path).unwrap();
        let found = taiyaku_library::dictionary::lookup(&conn, "hablaron").unwrap();
        assert_eq!(found[0].word, "hablar");
    }

    #[test]
    fn a_name_that_is_no_language_code_is_refused() {
        for lang in ["../es", "ES", "e", "spanish"] {
            assert!(
                import(lang, Path::new("x"), Path::new("y")).is_err(),
                "{lang}"
            );
        }
    }
}
