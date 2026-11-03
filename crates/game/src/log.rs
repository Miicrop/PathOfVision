use regex::Regex;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const TAIL_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub enum AreaKind {
    Map,
    Hideout,
    Town,
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Area {
    pub id: String,
    pub level: u32,
    pub kind: AreaKind,
}

fn area_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"Generating level (\d+) area "([^"]+)""#).unwrap())
}

pub fn classify(id: &str) -> AreaKind {
    if id.contains("Hideout") {
        AreaKind::Hideout
    } else if id.starts_with("Map") {
        AreaKind::Map
    } else if id.contains("_town") || id.ends_with("town") {
        AreaKind::Town
    } else {
        AreaKind::Other
    }
}

pub fn parse_area_line(line: &str) -> Option<Area> {
    let c = area_regex().captures(line)?;
    let level = c[1].parse().ok()?;
    let id = c[2].to_string();
    let kind = classify(&id);
    Some(Area { id, level, kind })
}

/// Existing log files, `LatestClient.txt` first, then `Client.txt`.
pub fn log_files(poe_dir: &Path) -> Vec<PathBuf> {
    ["logs/LatestClient.txt", "logs/Client.txt"]
        .iter()
        .map(|n| poe_dir.join(n))
        .filter(|p| p.is_file())
        .collect()
}

pub struct LogWatcher {
    path: PathBuf,
    /// Offset of the first unprocessed byte; `None` before the first successful read.
    pos: Option<u64>,
    current: Option<Area>,
    bytes_read: u64,
}

impl LogWatcher {
    pub fn new(path: PathBuf) -> LogWatcher {
        LogWatcher { path, pos: None, current: None, bytes_read: 0 }
    }

    /// Total bytes read from the file so far (for tests and diagnostics).
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    /// Returns the current area, or `None` while unknown (missing file, no area line yet).
    pub fn poll(&mut self) -> Option<Area> {
        match self.read_new() {
            Ok(()) => {}
            Err(_) => {
                // Missing/unreadable file: forget everything and start over next time.
                self.pos = None;
                self.current = None;
            }
        }
        self.current.clone()
    }

    fn read_new(&mut self) -> std::io::Result<()> {
        let mut file = std::fs::File::open(&self.path)?;
        let len = file.metadata()?.len();
        let mut start = match self.pos {
            Some(p) if p <= len => p,
            Some(_) => {
                // Truncated or rotated: re-read the new file from its beginning.
                self.current = None;
                0
            }
            None => 0,
        };
        let mut skip_first = false;
        if len - start > TAIL_BYTES {
            start = len - TAIL_BYTES;
            skip_first = true;
        }
        if start == len {
            self.pos = Some(len);
            return Ok(());
        }
        file.seek(SeekFrom::Start(start))?;
        let mut buf = Vec::with_capacity((len - start) as usize);
        file.take(len - start).read_to_end(&mut buf)?;
        self.bytes_read += buf.len() as u64;

        // Only whole lines; a trailing partial line waits for the next poll.
        let end = buf.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        self.pos = Some(start + end as u64);
        let mut text = String::from_utf8_lossy(&buf[..end]).into_owned();
        if skip_first {
            // The window may begin mid-line.
            text = text.split_once('\n').map(|(_, rest)| rest.to_string()).unwrap_or_default();
        }
        if let Some(area) = text.lines().rev().find_map(parse_area_line) {
            self.current = Some(area);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const MAP_LINE: &str = "2026/11/03 17:12:44 1678793843 1186a8a3 [DEBUG Client 47376] Generating level 83 area \"MapWorldsDunes\" with seed 3772491032";

    fn line(id: &str) -> String {
        format!("2026/11/03 17:12:44 1 2 [DEBUG Client 1] Generating level 1 area \"{id}\" with seed 5\n")
    }

    fn append(path: &std::path::Path, s: &str) {
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
        f.write_all(s.as_bytes()).unwrap();
    }

    #[test]
    fn parses_the_real_line_format() {
        assert_eq!(
            parse_area_line(MAP_LINE),
            Some(Area { id: "MapWorldsDunes".into(), level: 83, kind: AreaKind::Map })
        );
        assert_eq!(parse_area_line(&line("HideoutWorldTurtle")).unwrap().kind, AreaKind::Hideout);
        assert_eq!(parse_area_line(&line("2_11_endgame_town")).unwrap().kind, AreaKind::Town);
        assert_eq!(parse_area_line(&line("1_1_town")).unwrap().kind, AreaKind::Town);
        assert_eq!(parse_area_line(&line("1_1_2")).unwrap().kind, AreaKind::Other);
        assert_eq!(parse_area_line("2026/11/03 [INFO Client 1] @From Bob: what area is this"), None);
    }

    #[test]
    fn watcher_follows_appended_lines() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("c.txt");
        append(&p, &line("HideoutWorldTurtle"));
        let mut w = LogWatcher::new(p.clone());
        assert_eq!(w.poll().unwrap().kind, AreaKind::Hideout);
        append(&p, &line("MapWorldsDunes"));
        assert_eq!(w.poll().unwrap().kind, AreaKind::Map);
        assert_eq!(w.poll().unwrap().kind, AreaKind::Map);
    }

    #[test]
    fn reads_only_the_tail_of_a_large_log() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("c.txt");
        {
            let mut f = std::io::BufWriter::new(std::fs::File::create(&p).unwrap());
            let filler = "2026/11/03 17:12:44 1 2 [INFO Client 1] some chat filler text here\n";
            for _ in 0..(50 * 1024 * 1024 / filler.len()) {
                f.write_all(filler.as_bytes()).unwrap();
            }
            f.write_all(line("MapWorldsDunes").as_bytes()).unwrap();
        }
        let mut w = LogWatcher::new(p.clone());
        assert_eq!(w.poll().unwrap().kind, AreaKind::Map);
        assert!(w.bytes_read() <= 256 * 1024, "read {}", w.bytes_read());
        let extra = line("HideoutWorldTurtle");
        append(&p, &extra);
        assert_eq!(w.poll().unwrap().kind, AreaKind::Hideout);
        assert!(w.bytes_read() <= 256 * 1024 + extra.len() as u64);
    }

    #[test]
    fn missing_log_gives_unknown_area() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("later.txt");
        let mut w = LogWatcher::new(p.clone());
        assert_eq!(w.poll(), None);
        append(&p, &line("MapWorldsDunes"));
        assert_eq!(w.poll().unwrap().kind, AreaKind::Map);
    }

    #[test]
    fn truncated_log_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("c.txt");
        append(&p, &line("MapWorldsDunes"));
        append(&p, &line("MapWorldsDunes"));
        let mut w = LogWatcher::new(p.clone());
        assert_eq!(w.poll().unwrap().kind, AreaKind::Map);
        std::fs::write(&p, line("2_11_endgame_town")).unwrap();
        assert_eq!(w.poll().unwrap().kind, AreaKind::Town);
    }

    #[test]
    fn log_files_lists_existing_in_order() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("logs")).unwrap();
        std::fs::write(dir.path().join("logs/Client.txt"), "").unwrap();
        assert_eq!(log_files(dir.path()), vec![dir.path().join("logs/Client.txt")]);
        std::fs::write(dir.path().join("logs/LatestClient.txt"), "").unwrap();
        assert_eq!(log_files(dir.path()).len(), 2);
        assert!(log_files(dir.path())[0].ends_with("LatestClient.txt"));
    }
}
