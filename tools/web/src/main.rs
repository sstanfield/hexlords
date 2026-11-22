//! Hexlords for a browser: builds the game as WebAssembly and puts what a web
//! server has to give out into one directory, and can be that server.
//!
//! ```sh
//! cargo run -p web                 # build into target/web
//! cargo run -p web -- --serve      # and serve it: http://127.0.0.1:8080
//! ```
//!
//! The directory holds `index.html` (the page, `tools/web/index.html`),
//! `hexlords.wasm` (the game, with `assets/` inside it: `src/files.rs`),
//! `gl.js` (miniquad's loader, copied from the miniquad the game is built
//! with, so the two always match) and the licences that go with a copy of the
//! game. Any server of static files will do for it; a browser will not load
//! WebAssembly from a `file:` address, which is why there is one here.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// The page, as it is in the repository.
const PAGE: &str = include_str!("../index.html");
const TARGET: &str = "wasm32-unknown-unknown";
const DEFAULT_PORT: u16 = 8080;

const USAGE: &str = "usage: web [--out <dir>] [--debug] [--serve [<port>]]

Builds Hexlords for a browser into a directory (target/web).
  --out <dir>       the directory to fill instead
  --debug           an unoptimized build: quicker to make, slow and large
  --serve [<port>]  then serve the directory at http://127.0.0.1:<port> (8080)";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Args {
    out: Option<PathBuf>,
    debug: bool,
    serve: Option<u16>,
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => parsed.out = Some(PathBuf::from(args.next().ok_or("--out needs a directory")?)),
            "--debug" => parsed.debug = true,
            "--serve" => {
                // The port is optional: a number after it is one.
                let port = args.peek().and_then(|next| next.parse::<u16>().ok());
                if port.is_some() {
                    args.next();
                }
                parsed.serve = Some(port.unwrap_or(DEFAULT_PORT));
            }
            "-h" | "--help" => return Err(String::new()),
            _ => return Err(format!("unknown option {arg}")),
        }
    }
    Ok(parsed)
}

fn main() {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            if !message.is_empty() {
                eprintln!("web: {message}");
            }
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    let root = repository();
    let out = args.out.clone().unwrap_or_else(|| root.join("target").join("web"));
    if let Err(e) = build(&root, &out, args.debug) {
        eprintln!("web: {e}");
        std::process::exit(1);
    }
    println!("web: the game for a browser is in {}", out.display());
    match args.serve {
        Some(port) => {
            if let Err(e) = serve(&out, port) {
                eprintln!("web: --serve: {e}");
                std::process::exit(1);
            }
        }
        None => println!("web: serve that directory with any web server, or run again with --serve"),
    }
}

/// The repository: this tool is `tools/web` in it.
fn repository() -> PathBuf {
    let tool = Path::new(env!("CARGO_MANIFEST_DIR"));
    tool.parent().and_then(Path::parent).unwrap_or(tool).to_path_buf()
}

/// The cargo that runs this tool, or the one on the path.
fn cargo() -> Command {
    Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

/// Build the game for a browser and fill `out`.
fn build(root: &Path, out: &Path, debug: bool) -> Result<(), String> {
    let mut command = cargo();
    command.current_dir(root).args(["build", "--target", TARGET, "-p", "hexlords", "--bin", "hexlords"]);
    if !debug {
        command.arg("--release");
    }
    let status = command.status().map_err(|e| format!("could not run cargo: {e}"))?;
    if !status.success() {
        return Err(format!("the build failed (is the target there? rustup target add {TARGET})"));
    }
    let profile = if debug { "debug" } else { "release" };
    // Where cargo put it: its target directory may be somewhere else.
    let metadata = cargo_metadata(root)?;
    let target_dir =
        string_after(&metadata, 0, "target_directory").ok_or("cargo metadata names no target directory")?;
    let wasm = Path::new(&target_dir).join(TARGET).join(profile).join("hexlords.wasm");
    let miniquad = miniquad_dir(&metadata).ok_or("cargo metadata names no miniquad")?;

    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let copy = |from: &Path, name: &str| -> Result<(), String> {
        std::fs::copy(from, out.join(name)).map(|_| ()).map_err(|e| format!("{}: {e}", from.display()))
    };
    copy(&wasm, "hexlords.wasm")?;
    copy(&miniquad.join("js").join("gl.js"), "gl.js")?;
    // What has to go with a copy of the game (NOTICE), and with miniquad's
    // loader.
    copy(&root.join("LICENSE"), "LICENSE.txt")?;
    copy(&root.join("NOTICE"), "NOTICE.txt")?;
    copy(&miniquad.join("LICENSE-MIT"), "LICENSE-miniquad.txt")?;
    std::fs::write(out.join("index.html"), PAGE).map_err(|e| format!("{}: {e}", out.join("index.html").display()))
}

/// What `cargo metadata` says of the workspace, as built for a browser.
fn cargo_metadata(root: &Path) -> Result<String, String> {
    let output = cargo()
        .current_dir(root)
        .args(["metadata", "--format-version", "1", "--filter-platform", TARGET])
        .output()
        .map_err(|e| format!("could not run cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(format!("cargo metadata failed: {}", String::from_utf8_lossy(&output.stderr).trim()));
    }
    String::from_utf8(output.stdout).map_err(|_| "cargo metadata: not UTF-8".to_string())
}

/// The value of the first `"key":"..."` at or after `from` in a JSON text.
/// (The few strings wanted of `cargo metadata` do not need a JSON reader;
/// this one knows a string's escapes.)
fn string_after(json: &str, from: usize, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\":\"");
    let start = from + json.get(from..)?.find(&pattern)? + pattern.len();
    let mut value = String::new();
    let mut chars = json[start..].chars();
    loop {
        match chars.next()? {
            '"' => return Some(value),
            '\\' => match chars.next()? {
                'n' => value.push('\n'),
                't' => value.push('\t'),
                'u' => {
                    let code: String = chars.by_ref().take(4).collect();
                    value.push(char::from_u32(u32::from_str_radix(&code, 16).ok()?)?);
                }
                other => value.push(other),
            },
            c => value.push(c),
        }
    }
}

/// The directory of the miniquad the game is built with: its loader for the
/// browser (`js/gl.js`) is there.
fn miniquad_dir(metadata: &str) -> Option<PathBuf> {
    // A package's record begins with its name and version, and its manifest
    // comes after. (A dependency on it begins with the name and a source.)
    let package = metadata.find("{\"name\":\"miniquad\",\"version\":")?;
    let manifest = string_after(metadata, package, "manifest_path")?;
    Path::new(&manifest).parent().map(Path::to_path_buf)
}

/// The type a browser is told a file has. It will not compile WebAssembly as
/// it arrives unless that is `application/wasm`.
fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// The file a request's path names in `dir`: `/` is the page, and nothing
/// outside the directory or hidden in it is ever named.
fn file_for(dir: &Path, request_path: &str) -> Option<PathBuf> {
    let path = request_path.split(['?', '#']).next().unwrap_or("");
    let path = path.strip_prefix('/')?;
    if path.is_empty() {
        return Some(dir.join("index.html"));
    }
    let plain = |part: &str| !part.is_empty() && !part.starts_with('.') && !part.contains(['\\', ':', '%']);
    path.split('/').all(plain).then(|| path.split('/').fold(dir.to_path_buf(), |file, part| file.join(part)))
}

/// The answer to a request: its status line, the type and what is sent.
fn answer(dir: &Path, request_line: &str) -> (&'static str, &'static str, Vec<u8>) {
    let text = |status, words: &str| (status, "text/plain; charset=utf-8", words.as_bytes().to_vec());
    let mut parts = request_line.split_whitespace();
    let (Some(method), Some(path)) = (parts.next(), parts.next()) else {
        return text("400 Bad Request", "bad request\n");
    };
    if method != "GET" && method != "HEAD" {
        return text("405 Method Not Allowed", "only GET\n");
    }
    match file_for(dir, path).filter(|f| f.is_file()).and_then(|f| std::fs::read(&f).ok().map(|bytes| (f, bytes))) {
        Some((file, bytes)) => ("200 OK", content_type(&file), bytes),
        None => text("404 Not Found", "no such file\n"),
    }
}

/// Answer one connection: one request, then it is closed.
fn handle(dir: &Path, stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    // The rest of the request's head is read and not looked at.
    let mut line = String::new();
    while reader.read_line(&mut line)? > 2 {
        line.clear();
    }
    let (status, kind, body) = answer(dir, &request_line);
    let mut stream = stream;
    // (The files change with each build: a browser is not to keep them.)
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\n\
         Connection: close\r\n\r\n",
        body.len()
    )?;
    if !request_line.starts_with("HEAD ") {
        stream.write_all(&body)?;
    }
    stream.flush()
}

/// Serve `dir` to this computer's browsers until the program is stopped.
fn serve(dir: &Path, port: u16) -> Result<(), String> {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("port {port}: {e}"))?;
    println!("web: open http://127.0.0.1:{port} in a browser (Ctrl+C here stops the server)");
    serve_on(listener, dir);
    Ok(())
}

fn serve_on(listener: TcpListener, dir: &Path) {
    for stream in listener.incoming().flatten() {
        let dir = dir.to_path_buf();
        // A browser opens several connections at once.
        std::thread::spawn(move || {
            let _ = handle(&dir, stream);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn parse(args: &[&str]) -> Result<Args, String> {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn the_command_line() {
        assert_eq!(parse(&[]), Ok(Args::default()));
        assert_eq!(parse(&["--serve"]).unwrap().serve, Some(DEFAULT_PORT));
        assert_eq!(parse(&["--serve", "9000"]).unwrap().serve, Some(9000));
        // What follows --serve is a port only if it is a number.
        let args = parse(&["--serve", "--debug", "--out", "there"]).unwrap();
        assert_eq!(args, Args { out: Some(PathBuf::from("there")), debug: true, serve: Some(DEFAULT_PORT) });
        assert!(parse(&["--out"]).is_err());
        assert!(parse(&["--port"]).is_err());
        assert_eq!(parse(&["--help"]), Err(String::new()));
    }

    #[test]
    fn the_page_loads_the_game_with_miniquads_loader() {
        // The canvas is the one gl.js looks for, and the files are the ones
        // `build` writes beside the page.
        assert!(PAGE.contains("<canvas id=\"glcanvas\" tabindex=\"1\">"));
        // The canvas is opaque: the page does not show through where the
        // game draws with less than full alpha (land in the fog of war
        // did, with the page's loading note under it), and the wrapper is
        // in place before the loader asks for its context.
        let (opaque, loader) = (PAGE.find("alpha: false").unwrap(), PAGE.find("<script src=\"gl.js\">").unwrap());
        assert!(PAGE.contains("canvas.getContext = function") && opaque < loader);
        assert!(PAGE.contains("<script src=\"gl.js\"></script>"));
        assert!(PAGE.contains("load(\"hexlords.wasm\")"));
        // The right button is the game's.
        assert!(PAGE.contains("contextmenu"));
    }

    #[test]
    fn a_string_is_read_out_of_cargos_metadata() {
        let json = r#"{"packages":[{"name":"hexlords","version":"0.1.0","dependencies":[{"name":"miniquad","source":"registry","req":"=0.4.11"}],"manifest_path":"/work/Cargo.toml"},{"name":"miniquad","version":"0.4.11","manifest_path":"C:\\Users\\a \"b\"\\miniquad-0.4.11\\Cargo.toml"},{"name":"miniquad-sys","version":"1.0.0","manifest_path":"/other/Cargo.toml"}],"target_directory":"/work/t\u00e4rget"}"#;
        assert_eq!(string_after(json, 0, "target_directory").as_deref(), Some("/work/tärget"));
        assert_eq!(string_after(json, 0, "manifest_path").as_deref(), Some("/work/Cargo.toml"));
        assert_eq!(string_after(json, 0, "nothing"), None);
        // miniquad's own record: not the first one, not a dependency on it,
        // and not a package whose name only begins so.
        let manifest = "C:\\Users\\a \"b\"\\miniquad-0.4.11\\Cargo.toml";
        assert_eq!(miniquad_dir(json), Path::new(manifest).parent().map(Path::to_path_buf));
        assert_eq!(
            miniquad_dir(r#"{"packages":[{"name":"miniquad-sys","version":"1.0.0","manifest_path":"/x/Cargo.toml"}]}"#),
            None
        );
    }

    #[test]
    fn the_miniquad_the_game_is_built_with_has_the_loader() {
        // The real thing: cargo's metadata names miniquad, and its gl.js is
        // there to copy.
        let metadata = cargo_metadata(&repository()).unwrap();
        let dir = miniquad_dir(&metadata).expect("miniquad in cargo metadata");
        assert!(dir.join("js").join("gl.js").is_file(), "{}", dir.display());
        assert!(dir.join("LICENSE-MIT").is_file());
        let target = string_after(&metadata, 0, "target_directory").unwrap();
        assert!(Path::new(&target).is_dir(), "{target}");
    }

    #[test]
    fn a_file_has_its_type() {
        assert_eq!(content_type(Path::new("hexlords.wasm")), "application/wasm");
        assert_eq!(content_type(Path::new("index.html")), "text/html; charset=utf-8");
        assert_eq!(content_type(Path::new("gl.js")), "text/javascript; charset=utf-8");
        assert_eq!(content_type(Path::new("NOTICE.txt")), "text/plain; charset=utf-8");
        assert_eq!(content_type(Path::new("other")), "application/octet-stream");
    }

    #[test]
    fn a_request_names_a_file_in_the_directory_only() {
        let dir = Path::new("out");
        assert_eq!(file_for(dir, "/"), Some(dir.join("index.html")));
        assert_eq!(file_for(dir, "/?fresh=1"), Some(dir.join("index.html")));
        assert_eq!(file_for(dir, "/hexlords.wasm"), Some(dir.join("hexlords.wasm")));
        assert_eq!(file_for(dir, "/a/b.txt#top"), Some(dir.join("a").join("b.txt")));
        for outside in
            ["/../Cargo.toml", "/a/../../x", "/.git/config", "//etc/passwd", "/%2e%2e/x", "/c:/x", "/a\\b", "x"]
        {
            assert_eq!(file_for(dir, outside), None, "{outside}");
        }
    }

    /// A directory of our own, with a page and a file beside it.
    fn site(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hexlords-web-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.html"), "<p>the page</p>").unwrap();
        std::fs::write(dir.join("hexlords.wasm"), [0u8, 97, 115, 109]).unwrap();
        dir
    }

    #[test]
    fn a_request_is_answered() {
        let dir = site("answer");
        assert_eq!(
            answer(&dir, "GET / HTTP/1.1\r\n"),
            ("200 OK", "text/html; charset=utf-8", b"<p>the page</p>".to_vec())
        );
        assert_eq!(
            answer(&dir, "GET /hexlords.wasm HTTP/1.1\r\n"),
            ("200 OK", "application/wasm", vec![0, 97, 115, 109])
        );
        assert_eq!(answer(&dir, "GET /none.js HTTP/1.1\r\n").0, "404 Not Found");
        assert_eq!(answer(&dir, "GET /../x HTTP/1.1\r\n").0, "404 Not Found");
        assert_eq!(answer(&dir, "POST / HTTP/1.1\r\n").0, "405 Method Not Allowed");
        assert_eq!(answer(&dir, "\r\n").0, "400 Bad Request");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_server_gives_the_game_out_over_http() {
        let dir = site("serve");
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let served = dir.clone();
        std::thread::spawn(move || serve_on(listener, &served));
        let get = |path: &str| {
            let mut stream = TcpStream::connect(address).unwrap();
            write!(stream, "GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
            let mut reply = Vec::new();
            stream.read_to_end(&mut reply).unwrap();
            String::from_utf8_lossy(&reply).into_owned()
        };
        let page = get("/");
        assert!(page.starts_with("HTTP/1.1 200 OK\r\n"), "{page}");
        assert!(page.contains("Content-Type: text/html; charset=utf-8\r\n"));
        assert!(page.contains("Content-Length: 15\r\n"));
        assert!(page.ends_with("\r\n\r\n<p>the page</p>"));
        let wasm = get("/hexlords.wasm");
        assert!(wasm.contains("Content-Type: application/wasm\r\n"), "{wasm}");
        assert!(get("/nothing").starts_with("HTTP/1.1 404 Not Found\r\n"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
