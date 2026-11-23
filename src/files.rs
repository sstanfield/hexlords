//! Reading files: from the disk, or, in a browser, from the copy of `assets/`
//! the program carries inside itself.
//!
//! A browser has no files to read, so the WebAssembly build (`cargo run -p
//! web`) holds every file of `assets/` in a [`Table`] that `build.rs` lists,
//! and these functions look there; every other build goes to the disk, as
//! before. Whatever reads a file of the game's (a map, a picture, a layout)
//! asks here, not `std::fs`, and so works in both. The one thing written
//! is a saved game (`write_whole`, `remove`), on a disk: a browser build
//! has nowhere to write, and is never asked to (`platform::saves_dir`).

use std::io;
use std::path::{Component, Path, PathBuf};

/// Files kept in the program: each one's name (`assets/maps/example.map`,
/// with `/` between its parts on every system) and what is in it.
pub struct Table {
    files: &'static [(&'static str, &'static [u8])],
}

impl Table {
    pub const fn new(files: &'static [(&'static str, &'static [u8])]) -> Table {
        Table { files }
    }

    /// The names of all the files.
    pub fn names(&self) -> impl Iterator<Item = &'static str> {
        self.files.iter().map(|(name, _)| *name)
    }

    /// What is in a file.
    pub fn read(&self, path: &Path) -> Option<&'static [u8]> {
        let name = name_of(path);
        self.files.iter().find(|(n, _)| *n == name).map(|(_, bytes)| *bytes)
    }

    pub fn is_file(&self, path: &Path) -> bool {
        self.read(path).is_some()
    }

    /// A directory is what a file is in: there are no empty ones.
    pub fn is_dir(&self, path: &Path) -> bool {
        let dir = name_of(path);
        self.files.iter().any(|(name, _)| inside(name, &dir).is_some())
    }

    /// The files and directories in a directory, by name: each is `dir` with
    /// its own name joined on. `None` if there is no such directory.
    pub fn list(&self, dir: &Path) -> Option<Vec<PathBuf>> {
        let within = name_of(dir);
        let mut found: Vec<&str> = self
            .files
            .iter()
            .filter_map(|(name, _)| inside(name, &within))
            .map(|rest| rest.split('/').next().unwrap_or(rest))
            .collect();
        found.sort_unstable();
        found.dedup();
        (!found.is_empty()).then(|| found.into_iter().map(|entry| dir.join(entry)).collect())
    }
}

/// What is left of a file's name after the directory it is in (any level
/// down), if it is in it.
fn inside<'a>(name: &'a str, dir: &str) -> Option<&'a str> {
    if dir.is_empty() {
        return Some(name);
    }
    name.strip_prefix(dir)?.strip_prefix('/')
}

/// A path as the table names its files: the parts joined by `/`, with `.`
/// left out and `..` taking the part before it away.
fn name_of(path: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for part in path.components() {
        match part {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::ParentDir => {
                parts.pop();
            }
            Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
        }
    }
    parts.join("/")
}

/// The files of `assets/`, as `build.rs` found them when the program was
/// built.
#[cfg(any(test, target_arch = "wasm32"))]
static EMBEDDED: Table = Table::new(include!(concat!(env!("OUT_DIR"), "/embedded.rs")));

/// True where the files come from inside the program (a browser), and the
/// asset directory is the name `assets` and not a place on a disk.
pub const EMBEDDED_ASSETS: bool = cfg!(target_arch = "wasm32");

#[cfg(target_arch = "wasm32")]
fn missing() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "no such file")
}

/// What is in a file.
pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    #[cfg(target_arch = "wasm32")]
    return EMBEDDED.read(path).map(<[u8]>::to_vec).ok_or_else(missing);
    #[cfg(not(target_arch = "wasm32"))]
    std::fs::read(path)
}

/// What is in a file of text.
pub fn read_to_string(path: &Path) -> io::Result<String> {
    let bytes = read(path)?;
    String::from_utf8(bytes).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "not UTF-8 text"))
}

pub fn is_file(path: &Path) -> bool {
    #[cfg(target_arch = "wasm32")]
    return EMBEDDED.is_file(path);
    #[cfg(not(target_arch = "wasm32"))]
    path.is_file()
}

pub fn is_dir(path: &Path) -> bool {
    #[cfg(target_arch = "wasm32")]
    return EMBEDDED.is_dir(path);
    #[cfg(not(target_arch = "wasm32"))]
    path.is_dir()
}

/// The files and directories in a directory, by name.
pub fn list(dir: &Path) -> io::Result<Vec<PathBuf>> {
    #[cfg(target_arch = "wasm32")]
    return EMBEDDED.list(dir).ok_or_else(missing);
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut found = std::fs::read_dir(dir)?.map(|e| e.map(|e| e.path())).collect::<io::Result<Vec<PathBuf>>>()?;
        found.sort();
        Ok(found)
    }
}

/// Write a file, whole or not at all: beside it first, then put in its
/// place (what was there stays until then). Its directory is made if it is
/// not there. In a browser there is nowhere to write: an error.
pub fn write_whole(path: &Path, bytes: &[u8]) -> io::Result<()> {
    #[cfg(target_arch = "wasm32")]
    return Err(io::Error::new(
        io::ErrorKind::Unsupported,
        format!("{} bytes for {path:?}: nowhere to write", bytes.len()),
    ));
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        let mut beside = path.as_os_str().to_owned();
        beside.push(".part");
        let beside = PathBuf::from(beside);
        std::fs::write(&beside, bytes)?;
        std::fs::rename(&beside, path)
    }
}

/// Take a file away (one written with `write_whole`).
pub fn remove(path: &Path) -> io::Result<()> {
    #[cfg(target_arch = "wasm32")]
    return Err(io::Error::new(io::ErrorKind::Unsupported, format!("{path:?}: nothing was written")));
    #[cfg(not(target_arch = "wasm32"))]
    std::fs::remove_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    static SMALL: Table = Table::new(&[
        ("assets/maps/a.map", b"a"),
        ("assets/maps/land/assets/units/x.png", b"x"),
        ("assets/maps/land/terrain.map", b"land"),
        ("assets/skin/skin.tb.txt", b"skin"),
    ]);

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn a_file_is_written_whole_and_taken_away() {
        let dir = std::env::temp_dir().join(format!("hexlords-files-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("deep").join("a.txt");
        write_whole(&file, b"one").unwrap();
        write_whole(&file, b"two").unwrap();
        assert_eq!(read_to_string(&file).unwrap(), "two");
        // Nothing is left beside it.
        assert_eq!(list(&dir.join("deep")).unwrap(), std::slice::from_ref(&file));
        remove(&file).unwrap();
        assert!(!is_file(&file) && remove(&file).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_table_reads_a_file_by_its_path() {
        assert_eq!(SMALL.read(Path::new("assets/maps/a.map")), Some(&b"a"[..]));
        // However the path is put together.
        assert_eq!(SMALL.read(&Path::new("assets").join("maps").join("land").join("terrain.map")), Some(&b"land"[..]));
        assert_eq!(SMALL.read(Path::new("./assets/skin/../maps/a.map")), Some(&b"a"[..]));
        assert_eq!(SMALL.read(Path::new("assets/maps/b.map")), None);
        // A directory is not a file, and part of a name is neither.
        assert_eq!(SMALL.read(Path::new("assets/maps")), None);
        assert!(SMALL.is_file(Path::new("assets/skin/skin.tb.txt")));
        assert!(!SMALL.is_file(Path::new("assets/skin")));
    }

    #[test]
    fn a_table_knows_its_directories() {
        assert!(SMALL.is_dir(Path::new("assets")));
        assert!(SMALL.is_dir(Path::new("assets/maps/land")));
        assert!(SMALL.is_dir(Path::new("assets/maps/land/assets")));
        assert!(!SMALL.is_dir(Path::new("assets/maps/a.map")));
        assert!(!SMALL.is_dir(Path::new("assets/ma")));
        assert!(!SMALL.is_dir(Path::new("encumbered")));
    }

    #[test]
    fn a_table_lists_a_directory() {
        assert_eq!(SMALL.list(Path::new("assets")), Some(paths(&["assets/maps", "assets/skin"])));
        assert_eq!(SMALL.list(Path::new("assets/maps")), Some(paths(&["assets/maps/a.map", "assets/maps/land"])));
        assert_eq!(SMALL.list(Path::new("")), Some(paths(&["assets"])));
        assert_eq!(SMALL.list(Path::new("assets/maps/a.map")), None);
        assert_eq!(SMALL.list(Path::new("encumbered")), None);
    }

    /// Every file under a directory, as the table would name it.
    fn on_disk(dir: &Path, name: &str, found: &mut Vec<(String, PathBuf)>) {
        for path in list(dir).unwrap() {
            let file = path.file_name().unwrap().to_string_lossy().into_owned();
            if file.starts_with('.') {
                continue;
            }
            let name = format!("{name}/{file}");
            if path.is_dir() { on_disk(&path, &name, found) } else { found.push((name, path)) }
        }
    }

    #[test]
    fn the_program_carries_every_file_of_the_assets() {
        // What a browser build reads is what is in `assets/`: all of it, and
        // nothing from anywhere else (the owner's private maps least of all).
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut disk = Vec::new();
        on_disk(&root.join("assets"), "assets", &mut disk);
        disk.sort();
        let names: Vec<&str> = disk.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(EMBEDDED.names().collect::<Vec<_>>(), names);
        assert!(names.contains(&"assets/skin/skin.tb.txt"));
        for (name, path) in &disk {
            assert_eq!(EMBEDDED.read(Path::new(name)), Some(&std::fs::read(path).unwrap()[..]), "{name}");
        }
    }

    #[test]
    fn the_table_and_the_disk_say_the_same() {
        // The functions the game calls give the same answers from either.
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        for dir in ["assets", "assets/maps", "assets/maps/continent", "assets/tiles"] {
            let from_disk: Vec<PathBuf> = list(&root.join(dir))
                .unwrap()
                .iter()
                .filter(|p| !p.file_name().unwrap().to_string_lossy().starts_with('.'))
                .map(|p| p.strip_prefix(root).unwrap().to_path_buf())
                .collect();
            assert_eq!(EMBEDDED.list(Path::new(dir)), Some(from_disk), "{dir}");
        }
        for path in ["assets/maps/continent", "assets/maps/continent/terrain.map", "assets/maps/none", "assets/nothing"]
        {
            assert_eq!(EMBEDDED.is_dir(Path::new(path)), is_dir(&root.join(path)), "{path}");
            assert_eq!(EMBEDDED.is_file(Path::new(path)), is_file(&root.join(path)), "{path}");
        }
        assert!(read(&root.join("assets/nothing")).is_err());
        assert_eq!(read_to_string(&root.join("assets/maps/example.map")).unwrap().as_bytes(), {
            EMBEDDED.read(Path::new("assets/maps/example.map")).unwrap()
        });
    }
}
