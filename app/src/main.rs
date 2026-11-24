//! LeopardCraft native entry point and headless CLI.

use lc_core::luminance;
use lc_render::{PathTracer, RenderScene};
use lc_scene::Document;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let startup_warnings = match args.first() {
        None => recover_startup_artifacts(None),
        Some(path) if !path.starts_with('-') => recover_startup_artifacts(Some(Path::new(path))),
        Some(_) => Vec::new(),
    };
    let result = if args.is_empty() {
        launch_gui(None, None, startup_warnings)
    } else if args[0] == "--help" || args[0] == "-h" {
        print_help();
        Ok(())
    } else if args[0] == "--version" || args[0] == "-V" {
        println!("LeopardCraft {}", env!("CARGO_PKG_VERSION"));
        Ok(())
    } else if args[0] == "--headless" {
        run_headless(&args[1..])
    } else if args[0] == "--mcp" {
        run_mcp(&args[1..])
    } else if args[0] == "--mcp-http" {
        run_mcp_http(&args[1..])
    } else if args[0].starts_with('-') {
        Err(format!("unknown option '{}'; see --help", args[0]))
    } else {
        let path = PathBuf::from(&args[0]);
        match lc_io::import_with_report(&path) {
            Ok((doc, report)) => {
                let mut warnings = startup_warnings;
                if let Some(summary) = report.summary() {
                    warnings.push(summary);
                }
                launch_gui(Some(doc), Some(path), warnings)
            }
            Err(e) => Err(e),
        }
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn recover_startup_artifacts(scene: Option<&Path>) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut dirs = vec![PathBuf::from(".")];
    if let Some(parent) = scene.and_then(Path::parent) {
        if !parent.as_os_str().is_empty() && parent != Path::new(".") {
            dirs.push(parent.to_path_buf());
        }
    }
    for dir in dirs {
        match lc_io::obj::recover_interrupted_exports(&dir) {
            Ok(0) => {}
            Ok(count) => eprintln!(
                "Recovered {count} interrupted OBJ/MTL export(s) in {}",
                dir.display()
            ),
            Err(error) => eprintln!(
                "warning: failed to recover interrupted OBJ/MTL exports in {}: {error}",
                dir.display()
            ),
        }
        match lc_scene::recover_interrupted_saves(&dir) {
            Ok(0) => {}
            Ok(count) => eprintln!(
                "Recovered {count} interrupted scene save(s) in {}",
                dir.display()
            ),
            Err(error) => {
                let message = format!(
                    "failed to recover interrupted scene saves in {}: {error}",
                    dir.display()
                );
                eprintln!("warning: {message}");
                warnings.push(message);
            }
        }
    }
    warnings.extend(lc_scene::take_warnings());
    warnings
}

fn print_help() {
    println!(
        "LeopardCraft {}\n\
         Clean-room 3D modelling, rendering and animation.\n\n\
         Usage:\n\
           leopardcraft [scene.lcscene]        launch the editor\n\
           leopardcraft --headless render [options]\n\
           leopardcraft --headless export [options]\n\
           leopardcraft --headless selftest\n\
           leopardcraft --headless lifecycle\n\
           leopardcraft --mcp [--scene <path>]\n\
           leopardcraft --mcp-http [--addr <host:port>] [--scene <path>]\n\n\
         MCP options:\n\
           --scene <path>      scene to load (default: built-in studio)\n\
           --addr <host:port>  loopback address for --mcp-http (default: 127.0.0.1:8765)\n\n\
         Render options:\n\
           --scene <path>      scene file (default: built-in studio)\n\
           --out <path>        output .png or .exr (default: render.png)\n\
           --width <px>        image width\n\
           --height <px>       image height\n\
           --samples <n>       path samples per pixel\n\
           --seed <n>          deterministic seed\n\
           --time <seconds>    animation time\n\n\
         Export options:\n\
           --scene <path>      scene file (default: built-in studio)\n\
           --out <path>        .lcscene, .obj, .stl, .glb or .gltf\n",
        env!("CARGO_PKG_VERSION")
    );
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn run_headless(args: &[String]) -> Result<(), String> {
    let Some(command) = args.first() else {
        return Err("missing headless command (render, export or selftest)".into());
    };
    match command.as_str() {
        "render" => headless_render(&args[1..]),
        "export" => headless_export(&args[1..]),
        "selftest" => headless_selftest(&args[1..]),
        "lifecycle" => lc_ui::run_lifecycle_selftest(),
        "help" => {
            print_help();
            Ok(())
        }
        other => Err(format!("unknown headless command '{other}'")),
    }
}

fn startup_document(args: &[String]) -> Result<(Document, Option<PathBuf>), String> {
    match flag(args, "--scene") {
        Some(scene) => {
            let path = PathBuf::from(scene);
            let (doc, report) = lc_io::import_with_report(&path)?;
            if let Some(summary) = report.summary() {
                eprintln!("warning: {summary}");
            }
            Ok((doc, Some(path)))
        }
        None => Ok((Document::demo(), None)),
    }
}

fn run_mcp(args: &[String]) -> Result<(), String> {
    let (doc, path) = startup_document(args)?;
    lc_mcp::McpServer::new(doc, path).serve_stdio()
}

fn run_mcp_http(args: &[String]) -> Result<(), String> {
    let (doc, path) = startup_document(args)?;
    let addr = flag(args, "--addr").unwrap_or_else(|| lc_mcp::DEFAULT_HTTP_ADDR.to_string());
    lc_mcp::McpServer::new(doc, path).serve_http(&addr)
}

fn load_document(scene: Option<String>) -> Result<(Document, PathBuf), String> {
    match scene {
        Some(path) => {
            let path = PathBuf::from(path);
            let (doc, report) = lc_io::import_with_report(&path)?;
            if let Some(summary) = report.summary() {
                eprintln!("warning: {summary}");
            }
            Ok((doc, path))
        }
        None => Ok((Document::demo(), PathBuf::new())),
    }
}

fn headless_render(args: &[String]) -> Result<(), String> {
    let (doc, scene_path) = load_document(flag(args, "--scene"))?;
    let out = PathBuf::from(flag(args, "--out").unwrap_or_else(|| "render.png".into()));
    let mut width = doc.settings.render.width;
    let mut height = doc.settings.render.height;
    let mut samples = doc.settings.render.samples;
    let mut seed = doc.settings.render.seed;
    let mut time = 0.0f32;
    if let Some(v) = flag(args, "--width") {
        width = v.parse().map_err(|_| "invalid --width")?;
    }
    if let Some(v) = flag(args, "--height") {
        height = v.parse().map_err(|_| "invalid --height")?;
    }
    if let Some(v) = flag(args, "--samples") {
        samples = v.parse().map_err(|_| "invalid --samples")?;
    }
    if let Some(v) = flag(args, "--seed") {
        seed = v.parse().map_err(|_| "invalid --seed")?;
    }
    if let Some(v) = flag(args, "--time") {
        time = v.parse().map_err(|_| "invalid --time")?;
    }
    width = width.clamp(16, 8192);
    height = height.clamp(16, 8192);
    samples = samples.clamp(1, 4096);
    let base_dir = scene_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let started = Instant::now();
    let scene = RenderScene::from_document(&doc, time, base_dir)?;
    let mut tracer = PathTracer::new(width, height, seed);
    eprintln!(
        "Rendering {}×{} at {samples} spp, {} triangles, {} lights",
        width,
        height,
        scene.triangle_count(),
        scene.lights.len()
    );
    let mut done = 0u32;
    while done < samples {
        let chunk = (samples - done).min(8);
        tracer.render_pass(&scene, chunk);
        done += chunk;
        eprintln!("  {done}/{samples} spp");
    }
    let elapsed = started.elapsed();
    let is_exr = out
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("exr"))
        .unwrap_or(false);
    if is_exr {
        let pixels = tracer.hdr_pixels();
        lc_render::save_linear_exr(&out, &pixels, width, height)?;
    } else {
        lc_render::save_tracer_png(
            &out,
            &tracer,
            doc.settings.render.exposure,
            doc.settings.render.transparent_background,
        )?;
    }
    println!(
        "Rendered {}×{} ({} spp) to {} in {:.2?}",
        width,
        height,
        tracer.samples,
        out.display(),
        elapsed
    );
    Ok(())
}

fn headless_export(args: &[String]) -> Result<(), String> {
    let (doc, _) = load_document(flag(args, "--scene"))?;
    let out = PathBuf::from(flag(args, "--out").unwrap_or_else(|| "export.glb".into()));
    lc_io::export(&out, &doc)?;
    println!("Exported {} objects to {}", doc.scene.len(), out.display());
    Ok(())
}

fn headless_selftest(_args: &[String]) -> Result<(), String> {
    let started = Instant::now();
    let doc = Document::demo();
    let a = doc.evaluate(0.0);
    let b = doc.evaluate(0.0);
    if a.signature() != b.signature() {
        return Err("scene evaluation is not deterministic".into());
    }
    if a.active_camera.is_none() {
        return Err("demo scene has no active camera".into());
    }
    let json = lc_io::to_json(&doc)?;
    let restored = lc_io::from_json(&json)?;
    if restored.signature() != doc.signature() {
        return Err("native serialization round-trip changed the scene".into());
    }
    let triangle_count = a.triangle_count();
    if triangle_count == 0 {
        return Err("demo scene contains no renderable triangles".into());
    }
    let scene = RenderScene::from_document(&doc, 0.0, ".")?;
    let mut tracer = PathTracer::new(64, 36, 1234);
    tracer.render_pass(&scene, 2);
    let pixels = tracer.hdr_pixels();
    let total: f32 = pixels.iter().map(|p| luminance(*p)).sum();
    if !total.is_finite() || total <= 0.0 {
        return Err("path tracer produced a black or non-finite frame".into());
    }

    let dir = std::env::temp_dir().join(format!(
        "leopardcraft-selftest-{}-{}",
        std::process::id(),
        started.elapsed().as_nanos()
    ));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    lc_render::save_tracer_png(dir.join("test.png"), &tracer, 0.0, false)?;
    lc_render::save_linear_exr(dir.join("test.exr"), &pixels, 64, 36)?;

    let obj = dir.join("test.obj");
    lc_io::export(&obj, &doc)?;
    let obj_doc = lc_io::import(&obj)?;
    if obj_doc.evaluate(0.0).triangle_count() != triangle_count {
        return Err("OBJ round-trip changed the triangle count".into());
    }
    let glb = dir.join("test.glb");
    lc_io::export(&glb, &doc)?;
    let glb_doc = lc_io::import(&glb)?;
    if glb_doc.evaluate(0.0).triangle_count() != triangle_count {
        return Err("GLB round-trip changed the triangle count".into());
    }
    let stl = dir.join("test.stl");
    lc_io::export(&stl, &doc)?;
    let stl_doc = lc_io::import(&stl)?;
    let stl_tris = stl_doc.evaluate(0.0).triangle_count();
    if stl_tris == 0 {
        return Err("STL round-trip produced no geometry".into());
    }
    let _ = std::fs::remove_dir_all(&dir);

    println!(
        "selftest: OK — {} nodes, {} triangles, valid camera, deterministic evaluation,\n\
         native/OBJ/GLB/STL round-trips, PNG/EXR output, path-traced luminance {:.3}\n\
         elapsed {:.2?}",
        doc.scene.len(),
        triangle_count,
        total,
        started.elapsed()
    );
    Ok(())
}

fn launch_gui(
    initial: Option<Document>,
    path: Option<PathBuf>,
    startup_warnings: Vec<String>,
) -> Result<(), String> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1180.0, 760.0])
            .with_title("LeopardCraft"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "LeopardCraft",
        options,
        Box::new(move |cc| {
            let mut app = lc_ui::LeopardApp::new(cc);
            if let Some(doc) = initial {
                app.load_scene(doc, path);
            }
            app.report_scene_warnings(startup_warnings);
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| e.to_string())
}
