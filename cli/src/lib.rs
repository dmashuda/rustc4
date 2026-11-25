//! `rustc4`: architecture diagrams from source files, at the command line.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

use c4_model::{Severity, Source, Workspace};
use c4_render::SvgOptions;

mod mcp;
mod png;
mod serve;
mod site;

const USAGE: &str = "rustc4: C4-style architecture diagrams as code

Usage:
  rustc4 check  <path>...                       Report problems in a model
  rustc4 views  <path>...                       List the views
  rustc4 export <svg|png|json> <path>... [options]
                                                Write each view to a file
  rustc4 build  <path>... [-o <dir>]            Write a static site: the viewer,
                                                the model, and every view as SVG
  rustc4 serve  <path>... [--port <n>]          Show the model in a browser, and
                                                again whenever a file changes
  rustc4 bundle <path>... -o <file.json>        Gather a model into one file
  rustc4 query  <path>... -- <question>         Ask about a model (see below)
  rustc4 mcp    <path>...                       Serve the same questions to a
                                                language model, over MCP on stdio

A <path> is a .c4 (or .likec4) file, a directory searched for them, or a
bundle. Give several, one per repository, to read them as one model.

Questions for query:
  overview                      What is in the model
  search <words> [--kind k] [--tag t]
  element <name>                One element in full
  uses <name> [depth]           What it depends on
  used-by <name> [depth]        What depends on it
  path <from> <to>              How one reaches the other
  views | view <id>             A view, as text

Options:
  -o, --out <dir>       Where to write (default: the current directory; for build, ./dist)
  --view <id>           Only this view (may be repeated)
  --background <color>  Paint behind the diagram, e.g. '#ffffff' (default: transparent)
  --scale <n>           PNG pixels per diagram pixel (default: 2)
  --port <n>            Where serve listens (default: 8080; 0 for any free port)
  --no-legend           Leave out the legend: over each diagram, what its
                        shapes and lines mean, for those whose kind has a
                        'notation' in the specification
";

/// Runs the command line; returns the exit code.
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    match run_inner(args, out, err) {
        Ok(code) => code,
        Err(message) => {
            let _ = writeln!(err, "rustc4: {message}");
            2
        }
    }
}

struct Options {
    paths: Vec<PathBuf>,
    out: PathBuf,
    views: BTreeSet<String>,
    background: Option<String>,
    scale: f32,
    legend: bool,
    port: u16,
}

fn options(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        paths: Vec::new(),
        out: PathBuf::from("."),
        views: BTreeSet::new(),
        background: None,
        scale: 2.0,
        legend: true,
        port: 8080,
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "-o" | "--out" => o.out = PathBuf::from(value("--out")?),
            "--view" => {
                o.views.insert(value("--view")?);
            }
            "--background" => o.background = Some(value("--background")?),
            "--no-legend" => o.legend = false,
            "--scale" => {
                let text = value("--scale")?;
                o.scale = text
                    .parse()
                    .ok()
                    .filter(|s| *s > 0.0 && *s <= 8.0)
                    .ok_or(format!("'{text}' is not a scale: use a number up to 8"))?;
            }
            "--port" => {
                let text = value("--port")?;
                o.port =
                    text.parse().map_err(|_| format!("'{text}' is not a port: use a number up to 65535"))?;
            }
            flag if flag.starts_with('-') => return Err(format!("unknown option '{flag}'")),
            path => o.paths.push(PathBuf::from(path)),
        }
    }
    if o.paths.is_empty() {
        return Err("give at least one file or directory".into());
    }
    Ok(o)
}

fn is_source(path: &Path) -> bool {
    matches!(path.extension().and_then(|e| e.to_str()), Some("c4" | "likec4"))
}

fn collect(path: &Path, found: &mut Vec<PathBuf>) -> Result<(), String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.is_file() {
        found.push(path.to_path_buf());
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for entry in entries {
        let hidden = entry
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.') || n == "node_modules");
        if entry.is_dir() && !hidden {
            collect(&entry, found)?;
        } else if entry.is_file() && is_source(&entry) {
            found.push(entry);
        }
    }
    Ok(())
}

/// A file's name as the model knows it: with `/` between its parts, whatever
/// this platform writes there. The model finds the files a source names
/// (`icon`, `descriptionFile`) by that, a bundle made on one platform is read
/// on another, and Windows opens a path written either way.
fn slashed(name: &str, separator: char) -> String {
    if separator == '/' { name.to_string() } else { name.replace(separator, "/") }
}

/// Every source file under the paths given, in a fixed order.
pub fn read_sources(paths: &[PathBuf]) -> Result<Vec<Source>, String> {
    let mut sources = Vec::new();
    for path in paths {
        // A bundle stands for the files it was made from.
        if path.extension().is_some_and(|e| e == "json") {
            let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
            sources.extend(site::unbundle(&bytes).map_err(|e| format!("{}: {e}", path.display()))?);
            continue;
        }
        let mut files = Vec::new();
        collect(path, &mut files)?;
        for file in files {
            let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let name = slashed(&file.display().to_string(), std::path::MAIN_SEPARATOR);
            sources.push(Source { name, text });
        }
    }
    if sources.is_empty() {
        return Err("no .c4 files found".into());
    }
    // The images the sources name for icons, beside them. One that is not
    // there is left for the model to report, where it is asked for.
    for name in c4_model::load(&sources).wanted {
        sources.extend(std::fs::read(&name).ok().and_then(|bytes| Source::from_bytes(name, &bytes)));
    }
    Ok(sources)
}

fn plural(n: usize, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}

/// Prints the problems; returns whether any is an error.
fn report(ws: &Workspace, err: &mut dyn Write) -> bool {
    for diagnostic in &ws.diagnostics {
        let _ = writeln!(err, "{diagnostic}");
    }
    let errors = ws.diagnostics.iter().filter(|d| d.severity == Severity::Error).count();
    if errors > 0 {
        let _ = writeln!(err, "{errors} error{}", if errors == 1 { "" } else { "s" });
    }
    errors > 0
}

fn run_inner(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> Result<i32, String> {
    let Some(command) = args.first() else {
        let _ = write!(err, "{USAGE}");
        return Ok(2);
    };
    match command.as_str() {
        "-h" | "--help" | "help" => {
            let _ = write!(out, "{USAGE}");
            Ok(0)
        }
        "check" => {
            let o = options(&args[1..])?;
            let ws = c4_model::load(&read_sources(&o.paths)?);
            if report(&ws, err) {
                return Ok(1);
            }
            let m = &ws.model;
            let _ = writeln!(
                out,
                "ok: {} elements, {} relationships, {} views",
                m.elements.len(),
                m.relations.len(),
                m.views.len()
            );
            Ok(0)
        }
        "views" => {
            let o = options(&args[1..])?;
            let ws = c4_model::load(&read_sources(&o.paths)?);
            let failed = report(&ws, err);
            for view in &ws.model.views {
                let _ = writeln!(out, "{}\t{}", view.id, view.title);
            }
            Ok(i32::from(failed))
        }
        "export" => {
            let format = args.get(1).map(String::as_str).unwrap_or("");
            if !matches!(format, "svg" | "png" | "json") {
                return Err(format!("export what? 'svg', 'png' or 'json', not '{format}'"));
            }
            let o = options(&args[2..])?;
            let ws = c4_model::load(&read_sources(&o.paths)?);
            // Problems are reported, and whatever could be understood is
            // still drawn: a diagram with a gap says more than no diagram.
            let failed = report(&ws, err);
            for wanted in &o.views {
                if ws.model.view_def(wanted).is_none() {
                    return Err(format!(
                        "there is no view '{wanted}' (there are: {})",
                        ws.model.view_ids().join(", ")
                    ));
                }
            }
            std::fs::create_dir_all(&o.out).map_err(|e| format!("{}: {e}", o.out.display()))?;
            let renderer = if format == "png" { Some(png::Renderer::new()) } else { None };
            // Every written view, or just those asked for (which may be
            // the generated view of an element nobody wrote one for).
            let ids: Vec<&str> = if o.views.is_empty() {
                ws.model.view_ids()
            } else {
                o.views.iter().map(String::as_str).collect()
            };
            for id in ids {
                let view = ws.model.compute_view(id).expect("listed views exist");
                let diagram =
                    c4_layout::layout_with(&view, &ws.model.colors, c4_layout::Options { legend: o.legend });
                let svg = c4_render::svg(&diagram, &SvgOptions { background: o.background.clone() });
                // Nothing here fetches: an icon kept elsewhere is left out.
                let fetched = diagram
                    .nodes
                    .iter()
                    .filter_map(|n| n.icon.as_ref())
                    .filter(|i| !i.href.starts_with("data:"));
                if format == "png" && fetched.clone().next().is_some() {
                    let _ = writeln!(
                        err,
                        "warning: {id}.png is without {}: a PNG has only icons from SVG files, not from URLs.",
                        plural(fetched.count(), "icon")
                    );
                }
                let bytes = match (format, &renderer) {
                    ("svg", _) => svg.into_bytes(),
                    ("json", _) => serde_json::to_vec_pretty(&diagram).map_err(|e| e.to_string())?,
                    (_, Some(renderer)) => renderer.render(&svg, o.scale)?,
                    _ => unreachable!(),
                };
                let path = o.out.join(format!("{id}.{format}"));
                std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
                let _ = writeln!(out, "{}", path.display());
            }
            Ok(i32::from(failed))
        }
        "build" => {
            let mut o = options(&args[1..])?;
            if !args.iter().any(|a| a == "-o" || a == "--out") {
                o.out = PathBuf::from("dist");
            }
            let sources = read_sources(&o.paths)?;
            let ws = c4_model::load(&sources);
            let failed = report(&ws, err);
            let written = site::write(&o.out, &sources, &ws, o.legend)?;
            let _ = writeln!(
                out,
                "{}: {} files, {}",
                o.out.display(),
                written,
                plural(ws.model.views.len(), "view")
            );
            Ok(i32::from(failed))
        }
        "serve" => {
            let o = options(&args[1..])?;
            serve::Server::bind(o.paths, o.port, o.legend)?.run(out, err)
        }
        "bundle" => {
            let o = options(&args[1..])?;
            if o.out.extension().is_none_or(|e| e != "json") {
                return Err("bundle needs somewhere to write: -o <file.json>".into());
            }
            let sources = read_sources(&o.paths)?;
            let failed = report(&c4_model::load(&sources), err);
            if let Some(parent) = o.out.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            std::fs::write(&o.out, site::bundle(&sources)?)
                .map_err(|e| format!("{}: {e}", o.out.display()))?;
            let _ = writeln!(out, "{}: {}", o.out.display(), plural(sources.len(), "file"));
            Ok(i32::from(failed))
        }
        "query" => {
            let split = args
                .iter()
                .position(|a| a == "--")
                .ok_or("put the question after '--': rustc4 query <path>... -- overview")?;
            let paths: Vec<PathBuf> = args[1..split].iter().map(PathBuf::from).collect();
            if paths.is_empty() {
                return Err("give at least one file or directory".into());
            }
            let ws = c4_model::load(&read_sources(&paths)?);
            match mcp::answer(&ws, &mcp::question(&args[split + 1..])?) {
                Ok(text) => {
                    let _ = writeln!(out, "{text}");
                    Ok(0)
                }
                Err(text) => {
                    let _ = writeln!(err, "{text}");
                    Ok(1)
                }
            }
        }
        "mcp" => {
            let o = options(&args[1..])?;
            // Fail now, in words, rather than serve nothing.
            read_sources(&o.paths)?;
            mcp::serve(o.paths, &mut std::io::stdin().lock(), out);
            Ok(0)
        }
        other => Err(format!("unknown command '{other}' (try --help)")),
    }
}

#[cfg(test)]
#[path = "../tests/lib.rs"]
mod tests;
