//! The site of `rustc4 build`, served from memory and following the files:
//! the viewer, `model.json` and `views/<id>.svg` as the sources stand now,
//! and `changes`, an event stream that says when they have changed so the
//! page in the browser reads the model again.
//!
//! Only what the standard library has: a thread per connection, every
//! response closes it, and the files are watched by looking at them.

use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use c4_model::{Source, Workspace};

use crate::{collect, plural, read_sources, report, site};

/// How often the files are looked at.
const POLL: Duration = Duration::from_millis(250);
/// How long an event stream stays silent before a comment goes down it, which
/// is also how a page that has gone away is noticed.
const KEEP_ALIVE: Duration = Duration::from_secs(15);

/// The sources as last read.
struct Live {
    /// Counts the changes; what an event carries.
    version: u64,
    sources: Vec<Source>,
    /// `model.json`.
    bundle: Vec<u8>,
}

struct Shared {
    live: Mutex<Live>,
    changed: Condvar,
    legend: bool,
}

fn lock(live: &Mutex<Live>) -> MutexGuard<'_, Live> {
    live.lock().unwrap_or_else(|e| e.into_inner())
}

/// What the files looked like: each one's time of writing and length, `None`
/// for one that is not there.
type Stamp = Vec<(PathBuf, Option<(Option<SystemTime>, u64)>)>;

/// `named`: the files the sources name (icons, descriptions), there or not.
fn stamp(paths: &[PathBuf], named: &[String]) -> Stamp {
    let mut files = Vec::new();
    for path in paths {
        if collect(path, &mut files).is_err() {
            files.push(path.clone());
        }
    }
    files.extend(named.iter().map(PathBuf::from));
    files
        .into_iter()
        .map(|file| {
            let seen = std::fs::metadata(&file).ok().map(|m| (m.modified().ok(), m.len()));
            (file, seen)
        })
        .collect()
}

/// Every file a change to which changes the model, besides those under the
/// paths: the ones the sources name, read or still wanted.
fn named(sources: &[Source], ws: &Workspace) -> Vec<String> {
    sources.iter().map(|s| s.name.clone()).chain(ws.wanted.iter().cloned()).collect()
}

fn counts(ws: &Workspace) -> String {
    let m = &ws.model;
    format!(
        "{}, {}, {}",
        plural(m.elements.len(), "element"),
        plural(m.relations.len(), "relationship"),
        plural(m.views.len(), "view")
    )
}

pub struct Server {
    listener: TcpListener,
    paths: Vec<PathBuf>,
    shared: Arc<Shared>,
}

impl Server {
    /// Reads the model and takes the port (0: any free one) at the address
    /// to listen on. Fails in words if there is nothing to serve or nowhere
    /// to serve it.
    pub fn bind(paths: Vec<PathBuf>, host: IpAddr, port: u16, legend: bool) -> Result<Server, String> {
        let sources = read_sources(&paths)?;
        let bundle = site::bundle(&sources)?;
        let listener = TcpListener::bind((host, port))
            .map_err(|e| format!("port {port}: {e} (choose another with --port)"))?;
        let live = Live { version: 0, sources, bundle };
        Ok(Server {
            listener,
            paths,
            shared: Arc::new(Shared { live: Mutex::new(live), changed: Condvar::new(), legend }),
        })
    }

    pub fn addr(&self) -> Result<SocketAddr, String> {
        self.listener.local_addr().map_err(|e| e.to_string())
    }

    /// Serves, and reads the sources again whenever a file changes, until
    /// the process is stopped.
    pub fn run(self, out: &mut dyn Write, err: &mut dyn Write) -> Result<i32, String> {
        let addr = self.addr()?;
        let Server { listener, paths, shared } = self;
        let (mut watched, mut bundle) = {
            let live = lock(&shared.live);
            let ws = c4_model::load(&live.sources);
            report(&ws, err);
            let _ = writeln!(out, "http://{addr}: {}; following the files (ctrl-c stops)", counts(&ws));
            (named(&live.sources, &ws), live.bundle.clone())
        };
        let _ = out.flush();

        let serving = Arc::clone(&shared);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let shared = Arc::clone(&serving);
                // A page that went away mid-answer is nothing to report.
                std::thread::spawn(move || handle(stream, &shared));
            }
        });

        let mut before = stamp(&paths, &watched);
        loop {
            std::thread::sleep(POLL);
            let now = stamp(&paths, &watched);
            if now == before {
                continue;
            }
            match read_sources(&paths).and_then(|sources| Ok((site::bundle(&sources)?, sources))) {
                // Written again with the same in it.
                Ok((same, _)) if same == bundle => {}
                Ok((new, sources)) => {
                    let ws = c4_model::load(&sources);
                    report(&ws, err);
                    let _ = writeln!(out, "changed: {}", counts(&ws));
                    watched = named(&sources, &ws);
                    bundle = new.clone();
                    let mut live = lock(&shared.live);
                    live.version += 1;
                    live.sources = sources;
                    live.bundle = new;
                    shared.changed.notify_all();
                }
                // Half written, or gone: what was last read stays up.
                Err(message) => {
                    let _ = writeln!(err, "rustc4: {message}");
                }
            }
            let _ = out.flush();
            // Looked at again, since what the sources name may have changed.
            before = stamp(&paths, &watched);
        }
    }
}

fn content_type(name: &str) -> &'static str {
    match name.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        // Must be this, or the browser will not compile it as it arrives.
        Some("wasm") => "application/wasm",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        _ => "text/plain; charset=utf-8",
    }
}

struct Response<'a> {
    status: &'a str,
    content_type: &'a str,
    /// More header lines, each ending in `\r\n`.
    headers: &'a str,
    body: &'a [u8],
}

fn handle(stream: TcpStream, shared: &Shared) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut words = line.split_whitespace();
    let (method, target) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
    let mut header = String::new();
    while reader.read_line(&mut header)? > 0 && !header.trim().is_empty() {
        header.clear();
    }
    let mut stream = stream;
    let path = target.split(['?', '#']).next().unwrap_or("");
    let text = |status, body: &'static str| Response {
        status,
        content_type: "text/plain; charset=utf-8",
        headers: "",
        body: body.as_bytes(),
    };
    let head = method == "HEAD";
    if !head && method != "GET" {
        return respond(&mut stream, false, &text("405 Method Not Allowed", "only GET\n"));
    }
    if path == "/changes" && !head {
        return changes(&mut stream, shared);
    }
    if path == "/model.json" {
        let bundle = lock(&shared.live).bundle.clone();
        return respond(
            &mut stream,
            head,
            &Response {
                status: "200 OK",
                content_type: content_type(path),
                // Where the page hears that there is a newer one.
                headers: "x-rustc4-changes: changes\r\n",
                body: &bundle,
            },
        );
    }
    if let Some(id) = path.strip_prefix("/views/").and_then(|file| file.strip_suffix(".svg")) {
        let sources = lock(&shared.live).sources.clone();
        return match site::svg(&c4_model::load(&sources), id, shared.legend) {
            Some(svg) => respond(
                &mut stream,
                head,
                &Response {
                    status: "200 OK",
                    content_type: content_type(path),
                    headers: "",
                    body: svg.as_bytes(),
                },
            ),
            None => respond(&mut stream, head, &text("404 Not Found", "there is no such view\n")),
        };
    }
    let name = if path == "/" { "index.html" } else { path.trim_start_matches('/') };
    match site::VIEWER.iter().find(|(file, _)| *file == name) {
        Some((file, bytes)) => respond(
            &mut stream,
            head,
            &Response { status: "200 OK", content_type: content_type(file), headers: "", body: bytes },
        ),
        None => respond(&mut stream, head, &text("404 Not Found", "not found\n")),
    }
}

fn respond(stream: &mut TcpStream, head: bool, response: &Response) -> std::io::Result<()> {
    // Nothing is kept by the browser: the next request may have another answer.
    write!(
        stream,
        "HTTP/1.1 {}\r\ncontent-type: {}\r\ncontent-length: {}\r\ncache-control: no-store\r\n{}connection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len(),
        response.headers
    )?;
    if !head {
        stream.write_all(response.body)?;
    }
    stream.flush()
}

/// The event stream: one event now, so a page that was away catches up, and
/// one for every change after. Ends when the page stops listening.
fn changes(stream: &mut TcpStream, shared: &Shared) -> std::io::Result<()> {
    stream.write_all(
        b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-store\r\nconnection: close\r\n\r\n",
    )?;
    let mut told = None;
    loop {
        let version = {
            let mut live = lock(&shared.live);
            if told == Some(live.version) {
                live = shared.changed.wait_timeout(live, KEEP_ALIVE).unwrap_or_else(|e| e.into_inner()).0;
            }
            live.version
        };
        if told == Some(version) {
            stream.write_all(b": still here\n\n")?;
        } else {
            write!(stream, "data: {version}\n\n")?;
            told = Some(version);
        }
        stream.flush()?;
    }
}

#[cfg(test)]
#[path = "../tests/serve.rs"]
mod tests;
