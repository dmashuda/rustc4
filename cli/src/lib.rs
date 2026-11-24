//! `rustc4`: architecture diagrams from source files, at the command line.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

use c4_model::{Severity, Source, Workspace};
use c4_render::SvgOptions;

mod png;
mod site;

const USAGE: &str = "rustc4: C4-style architecture diagrams as code

Usage:
  rustc4 check  <path>...                       Report problems in a model
  rustc4 views  <path>...                       List the views
  rustc4 export <svg|png|json> <path>... [options]
                                                Write each view to a file
  rustc4 build  <path>... [-o <dir>]            Write a static site: the viewer,
                                                the model, and every view as SVG

A <path> is a .c4 (or .likec4) file, or a directory searched for them.

Options:
  -o, --out <dir>       Where to write (default: the current directory; for build, ./dist)
  --view <id>           Only this view (may be repeated)
  --background <color>  Paint behind the diagram, e.g. '#ffffff' (default: transparent)
  --scale <n>           PNG pixels per diagram pixel (default: 2)
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
}

fn options(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        paths: Vec::new(),
        out: PathBuf::from("."),
        views: BTreeSet::new(),
        background: None,
        scale: 2.0,
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
            "--scale" => {
                let text = value("--scale")?;
                o.scale = text
                    .parse()
                    .ok()
                    .filter(|s| *s > 0.0 && *s <= 8.0)
                    .ok_or(format!("'{text}' is not a scale: use a number up to 8"))?;
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

/// Every source file under the paths given, in a fixed order.
pub fn read_sources(paths: &[PathBuf]) -> Result<Vec<Source>, String> {
    let mut files = Vec::new();
    for path in paths {
        collect(path, &mut files)?;
    }
    if files.is_empty() {
        return Err("no .c4 files found".into());
    }
    files
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(Source { name: path.display().to_string(), text })
        })
        .collect()
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
            for id in ws.model.view_ids() {
                if !o.views.is_empty() && !o.views.contains(id) {
                    continue;
                }
                let view = ws.model.compute_view(id).expect("listed views exist");
                let diagram = c4_layout::layout(&view, &ws.model.colors);
                let svg = c4_render::svg(&diagram, &SvgOptions { background: o.background.clone() });
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
            let written = site::write(&o.out, &sources, &ws)?;
            let _ = writeln!(
                out,
                "{}: {} files, {}",
                o.out.display(),
                written,
                plural(ws.model.views.len(), "view")
            );
            Ok(i32::from(failed))
        }
        other => Err(format!("unknown command '{other}' (try --help)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rustc4(args: &[&str]) -> (i32, String, String) {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run(&args, &mut out, &mut err);
        (code, String::from_utf8(out).unwrap(), String::from_utf8(err).unwrap())
    }

    /// A fresh directory under the test's own temporary one.
    fn scratch(name: &str) -> PathBuf {
        let dir = PathBuf::from(
            std::env::var("TEST_TMPDIR").unwrap_or_else(|_| std::env::temp_dir().display().to_string()),
        )
        .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn checks_and_lists() {
        assert_eq!(
            rustc4(&["check", "examples"]),
            (0, "ok: 14 elements, 15 relationships, 4 views\n".into(), String::new())
        );
        let (code, out, _) = rustc4(&["views", "examples/shop.c4"]);
        assert_eq!(code, 0);
        assert_eq!(out.lines().next(), Some("index\tLandscape"));
        assert_eq!(out.lines().count(), 4);
    }

    #[test]
    fn problems_fail_the_check_with_a_place() {
        let dir = scratch("broken");
        std::fs::write(dir.join("m.c4"), "specification { element box }\nmodel {\n  a = box\n  a -> b\n}\n")
            .unwrap();
        let (code, out, err) = rustc4(&["check", dir.to_str().unwrap()]);
        assert_eq!((code, out.as_str()), (1, ""));
        assert!(err.contains("m.c4:4:8: error: There is no element 'b'."), "{err}");
        assert!(err.ends_with("1 error\n"));
    }

    #[test]
    fn exports_every_view_in_each_format() {
        let dir = scratch("export");
        let out_dir = dir.to_str().unwrap();
        let (code, out, err) = rustc4(&["export", "svg", "examples", "-o", out_dir]);
        assert_eq!((code, err.as_str()), (0, ""));
        assert_eq!(out.lines().count(), 4);
        let svg = std::fs::read_to_string(dir.join("shop.svg")).unwrap();
        assert!(svg.starts_with("<svg ") && svg.contains("Storefront"));

        let (code, _, _) = rustc4(&["export", "json", "examples", "-o", out_dir, "--view", "api"]);
        assert_eq!(code, 0);
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("api.json")).unwrap()).unwrap();
        assert_eq!(json["id"], "api");
        assert!(json["nodes"].as_array().unwrap().len() > 4);
        assert!(!dir.join("shop.json").exists(), "only the view asked for");

        let (code, _, err) =
            rustc4(&["export", "png", "examples", "-o", out_dir, "--view", "index", "--scale", "1"]);
        assert_eq!((code, err.as_str()), (0, ""));
        let png = std::fs::read(dir.join("index.png")).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn builds_a_site() {
        let dir = scratch("site");
        let (code, out, err) = rustc4(&["build", "examples", "-o", dir.to_str().unwrap()]);
        assert_eq!((code, err.as_str()), (0, ""));
        assert!(out.ends_with(": 9 files, 4 views\n"), "{out}");
        for file in [
            "index.html",
            "style.css",
            "main.js",
            "rustc4_bg.wasm",
            "model.json",
            "views/index.svg",
            "views/orders.svg",
        ] {
            assert!(dir.join(file).is_file(), "{file}");
        }
        assert_eq!(&std::fs::read(dir.join("rustc4_bg.wasm")).unwrap()[..4], b"\0asm");
        let model: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("model.json")).unwrap()).unwrap();
        assert_eq!(model["sources"][0]["name"], "shop.c4");
        assert!(model["sources"][0]["text"].as_str().unwrap().contains("specification"));
    }

    #[test]
    fn says_what_is_wrong_with_the_command() {
        assert_eq!(rustc4(&[]).0, 2);
        assert_eq!(rustc4(&["--help"]).0, 0);
        let (code, _, err) = rustc4(&["export", "gif", "examples"]);
        assert_eq!((code, err.as_str()), (2, "rustc4: export what? 'svg', 'png' or 'json', not 'gif'\n"));
        assert_eq!(rustc4(&["check"]).2, "rustc4: give at least one file or directory\n");
        assert!(rustc4(&["check", "nowhere"]).2.starts_with("rustc4: nowhere: "));
        let (code, _, err) = rustc4(&["export", "svg", "examples", "--view", "nope"]);
        assert_eq!(code, 2);
        assert_eq!(err, "rustc4: there is no view 'nope' (there are: index, shop, api, orders)\n");
        assert_eq!(rustc4(&["frobnicate"]).2, "rustc4: unknown command 'frobnicate' (try --help)\n");
    }
}
