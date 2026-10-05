//! Where logs come from: single files, archives and databases, read on
//! several threads.
//!
//! [`collect`] turns command-line paths into [`Source`]s, and [`for_each`]
//! reads every log in them, hands each to a worker thread, and passes the
//! workers' results back to the caller one at a time.
//!
//! Supported inputs (directories are searched recursively):
//!
//! - `.mjlog`, `.xml`, `.gz`: one log, plain or gzipped.
//! - `.tar`, `.tar.gz`, `.tgz`, `.tar.zst`, `.tzst`: an archive of logs,
//!   streamed without unpacking. Each entry may itself be plain, gzip,
//!   bzip2 or zstd compressed; entries that aren't logs are skipped.
//! - `.db`, `.sqlite`: an SQLite database with one log per row, in the
//!   layout of the phoenix-logs scraper (table `logs`, columns `log_id`
//!   and `log_content`, the content bzip2 compressed).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

/// One input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    File(PathBuf),
    Tar(PathBuf),
    Db(PathBuf),
}

impl Source {
    fn of(path: &Path) -> Option<Source> {
        let name = path.file_name()?.to_str()?.to_ascii_lowercase();
        let p = path.to_path_buf();
        let ends = |s: &[&str]| s.iter().any(|e| name.ends_with(e));
        if ends(&[".tar", ".tar.gz", ".tgz", ".tar.zst", ".tzst", ".tar.zstd"]) {
            Some(Source::Tar(p))
        } else if ends(&[".db", ".sqlite", ".sqlite3"]) {
            Some(Source::Db(p))
        } else if ends(&[".mjlog", ".xml", ".gz"]) {
            Some(Source::File(p))
        } else {
            None
        }
    }
}

/// The inputs under `paths`, sorted. Files named directly that aren't a
/// log, archive or database are reported and skipped.
pub fn collect(paths: &[PathBuf]) -> Vec<Source> {
    fn walk(path: &Path, out: &mut Vec<Source>) {
        if path.is_dir() {
            let Ok(entries) = std::fs::read_dir(path) else {
                eprintln!("can't read {}", path.display());
                return;
            };
            for e in entries.flatten() {
                walk(&e.path(), out);
            }
        } else if let Some(s) = Source::of(path) {
            out.push(s);
        }
    }
    let mut out = Vec::new();
    for p in paths {
        if p.is_file() && Source::of(p).is_none() {
            eprintln!("skipping {}: not a log, archive or database", p.display());
            continue;
        }
        walk(p, &mut out);
    }
    out.sort_by(|a, b| key(a).cmp(key(b)));
    out
}

fn key(s: &Source) -> &Path {
    match s {
        Source::File(p) | Source::Tar(p) | Source::Db(p) => p,
    }
}

/// Decompresses `bytes` if they start with a gzip, bzip2 or zstd header,
/// and returns the text.
pub fn decode(bytes: &[u8]) -> std::io::Result<String> {
    let mut s = String::new();
    if bytes.starts_with(&[0x1f, 0x8b]) {
        flate2::read::MultiGzDecoder::new(bytes).read_to_string(&mut s)?;
    } else if bytes.starts_with(b"BZh") {
        bzip2::read::MultiBzDecoder::new(bytes).read_to_string(&mut s)?;
    } else if bytes.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        zstd::stream::read::Decoder::new(bytes)?.read_to_string(&mut s)?;
    } else {
        return String::from_utf8(bytes.to_vec())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e));
    }
    Ok(s)
}

/// One log as read: where it came from, and its (still compressed) bytes.
struct Raw {
    name: String,
    bytes: Result<Vec<u8>, String>,
}

/// Reads every log in `sources` on `threads` worker threads. `work` gets
/// each log's name and text (or why it couldn't be read) on a worker;
/// `sink` gets each log's name and result on the calling thread, in no
/// particular order. A source that can't be opened reaches `work` as an
/// error under the source's own name.
pub fn for_each<R, W, S>(sources: &[Source], threads: usize, work: W, mut sink: S)
where
    R: Send,
    W: Fn(&str, Result<String, String>) -> R + Sync,
    S: FnMut((String, R)),
{
    let threads = threads.max(1);
    let (raw_tx, raw_rx) = mpsc::sync_channel::<Raw>(threads * 4);
    let raw_rx = Arc::new(Mutex::new(raw_rx));
    let (out_tx, out_rx) = mpsc::channel::<(String, R)>();
    std::thread::scope(|scope| {
        scope.spawn(move || {
            for s in sources {
                read_source(s, &raw_tx);
            }
        });
        for _ in 0..threads {
            let rx = Arc::clone(&raw_rx);
            let tx = out_tx.clone();
            let work = &work;
            scope.spawn(move || {
                loop {
                    let next = rx.lock().unwrap().recv();
                    let Ok(raw) = next else { break };
                    let text = raw
                        .bytes
                        .and_then(|b| decode(&b).map_err(|e| e.to_string()));
                    let r = work(&raw.name, text);
                    if tx.send((raw.name, r)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(out_tx);
        for r in out_rx {
            sink(r);
        }
    });
}

fn read_source(s: &Source, tx: &mpsc::SyncSender<Raw>) {
    let send = |name: String, bytes: Result<Vec<u8>, String>| {
        let _ = tx.send(Raw { name, bytes });
    };
    match s {
        Source::File(p) => send(
            p.display().to_string(),
            std::fs::read(p).map_err(|e| e.to_string()),
        ),
        Source::Tar(p) => {
            if let Err(e) = read_tar(p, tx) {
                send(p.display().to_string(), Err(e.to_string()));
            }
        }
        Source::Db(p) => {
            if let Err(e) = read_db(p, tx) {
                send(p.display().to_string(), Err(e));
            }
        }
    }
}

fn read_tar(path: &Path, tx: &mpsc::SyncSender<Raw>) -> std::io::Result<()> {
    let file = std::io::BufReader::new(std::fs::File::open(path)?);
    let name = path.to_string_lossy().to_ascii_lowercase();
    let reader: Box<dyn Read> =
        if name.ends_with(".zst") || name.ends_with(".tzst") || name.ends_with(".zstd") {
            Box::new(zstd::stream::read::Decoder::with_buffer(file)?)
        } else if name.ends_with(".gz") || name.ends_with(".tgz") {
            Box::new(flate2::read::MultiGzDecoder::new(file))
        } else {
            Box::new(file)
        };
    let mut archive = tar::Archive::new(reader);
    let (mut skipped, mut first_skipped) = (0usize, None);
    for entry in archive.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let entry_name = entry.path()?.to_string_lossy().into_owned();
        if !looks_like_log(&entry_name) {
            skipped += 1;
            if first_skipped.is_none() {
                first_skipped = Some(entry_name);
            }
            continue;
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes)?;
        if tx
            .send(Raw {
                name: format!("{}:{entry_name}", path.display()),
                bytes: Ok(bytes),
            })
            .is_err()
        {
            break;
        }
    }
    if let Some(first) = first_skipped {
        eprintln!(
            "{}: skipped {skipped} entries that don't look like logs (first: {first})",
            path.display()
        );
    }
    Ok(())
}

/// An archive entry worth parsing: a log extension, possibly compressed,
/// or no extension at all (some archives name logs by id only).
fn looks_like_log(name: &str) -> bool {
    let file = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
    let mut base = file.as_str();
    for ext in [".gz", ".bz2", ".zst", ".zstd"] {
        base = base.strip_suffix(ext).unwrap_or(base);
    }
    base.ends_with(".mjlog") || base.ends_with(".xml") || !base.contains('.') || base != file
}

fn read_db(path: &Path, tx: &mpsc::SyncSender<Raw>) -> Result<(), String> {
    let db =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
    let mut stmt = db
        .prepare("SELECT log_id, log_content FROM logs WHERE log_content IS NOT NULL")
        .map_err(|e| {
            format!(
                "{e}: expected table `logs` with columns `log_id` and `log_content`; \
                 this database has: {}",
                schema(&db)
            )
        })?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let id: String = row.get(0).map_err(|e| e.to_string())?;
        // Stored as a blob by the scraper, but accept text too.
        let bytes = match row.get_ref(1).map_err(|e| e.to_string())? {
            rusqlite::types::ValueRef::Blob(b) => b.to_vec(),
            rusqlite::types::ValueRef::Text(t) => t.to_vec(),
            other => {
                let _ = tx.send(Raw {
                    name: format!("{}:{id}", path.display()),
                    bytes: Err(format!("log_content is {:?}", other.data_type())),
                });
                continue;
            }
        };
        if tx
            .send(Raw {
                name: format!("{}:{id}", path.display()),
                bytes: Ok(bytes),
            })
            .is_err()
        {
            break;
        }
    }
    Ok(())
}

/// The `CREATE` statements of a database's tables, for error messages.
fn schema(db: &rusqlite::Connection) -> String {
    let sql: Result<Vec<String>, _> = db
        .prepare("SELECT sql FROM sqlite_master WHERE type = 'table'")
        .and_then(|mut st| st.query_map([], |r| r.get(0))?.collect());
    match sql {
        Ok(v) if !v.is_empty() => v.join("; "),
        Ok(_) => "no tables".into(),
        Err(e) => format!("unreadable schema ({e})"),
    }
}

/// Worker threads to use by default: one per core.
pub fn default_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const LOG: &str = "<mjloggm ver=\"2.3\"></mjloggm>";

    fn dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("usagi-source-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn read_all(sources: &[Source]) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for_each(sources, 3, |_, text| text.unwrap(), |r| out.push(r));
        out.sort();
        out
    }

    #[test]
    fn decode_detects_compression() {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(LOG.as_bytes()).unwrap();
        let gz = gz.finish().unwrap();
        let mut bz = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
        bz.write_all(LOG.as_bytes()).unwrap();
        let bz = bz.finish().unwrap();
        let zs = zstd::encode_all(LOG.as_bytes(), 1).unwrap();
        for bytes in [LOG.as_bytes(), &gz, &bz, &zs] {
            assert_eq!(decode(bytes).unwrap(), LOG);
        }
    }

    #[test]
    fn reads_tar_zst_and_db() {
        let d = dir();
        // A .tar.zst holding a plain log, a gzipped log and a stray file.
        let tar_path = d.join("logs.tar.zst");
        {
            let enc =
                zstd::stream::write::Encoder::new(std::fs::File::create(&tar_path).unwrap(), 1)
                    .unwrap()
                    .auto_finish();
            let mut b = tar::Builder::new(enc);
            let mut add = |name: &str, data: &[u8]| {
                let mut h = tar::Header::new_gnu();
                h.set_size(data.len() as u64);
                h.set_mode(0o644);
                h.set_cksum();
                b.append_data(&mut h, name, data).unwrap();
            };
            add("2009/a.mjlog", LOG.as_bytes());
            let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            gz.write_all(LOG.as_bytes()).unwrap();
            add("2009/b.mjlog.gz", &gz.finish().unwrap());
            add("README.txt", b"not a log");
            b.finish().unwrap();
        }
        // A phoenix-logs style database.
        let db_path = d.join("2009.db");
        let _ = std::fs::remove_file(&db_path);
        {
            let db = rusqlite::Connection::open(&db_path).unwrap();
            db.execute_batch(
                "CREATE TABLE logs (log_id TEXT PRIMARY KEY, date TEXT, is_tonpusen INT, \
                 is_sanma INT, is_processed INT, was_error INT, log_content TEXT);",
            )
            .unwrap();
            let mut bz = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
            bz.write_all(LOG.as_bytes()).unwrap();
            db.execute(
                "INSERT INTO logs (log_id, log_content) VALUES ('c', ?1), ('d', NULL)",
                [bz.finish().unwrap()],
            )
            .unwrap();
        }
        let sources = collect(std::slice::from_ref(&d));
        assert_eq!(sources, vec![Source::Db(db_path), Source::Tar(tar_path)]);
        let got = read_all(&sources);
        let mut names: Vec<&str> = got
            .iter()
            .map(|(n, _)| n.rsplit(':').next().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["2009/a.mjlog", "2009/b.mjlog.gz", "c"]);
        assert!(got.iter().all(|(_, t)| t == LOG));
        std::fs::remove_dir_all(&d).unwrap();
    }
}
