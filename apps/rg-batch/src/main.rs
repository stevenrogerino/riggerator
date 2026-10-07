//! rg-batch — headless Riggerator (like mayabatch / mayapy).
//!
//!   rg-batch [-file scene.rgs|.ma|.obj] [-script file.mel] [-command "mel"] [-render out.png]

use rg_core::{History, Scene};
use rg_script::{Host, Interpreter};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("usage: rg-batch [-file scene] [-script file.mel] [-command \"mel\"] [-render out.png] [-save out.rgs]");
        std::process::exit(if args.is_empty() { 1 } else { 0 });
    }
    let mut scene = Scene::new();
    let mut history = History::new();
    let mut file: Option<PathBuf> = None;
    let mut interp = Interpreter::new();
    let mut i = 0;
    let mut code = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        let val = args.get(i + 1).cloned();
        i += 2;
        let result: Result<(), String> = (|| {
            let v = val.ok_or_else(|| format!("{flag} needs a value"))?;
            match flag {
                "-file" | "-f" => {
                    let p = PathBuf::from(&v);
                    scene = if p.extension().is_some_and(|e| e == "rgs") {
                        rg_io::load_scene(&p)?
                    } else {
                        let mut s = Scene::new();
                        rg_io::import_file(&mut s, &p)?;
                        s
                    };
                    file = Some(p);
                    Ok(())
                }
                "-script" | "-s" | "-command" | "-c" => {
                    let src = if flag.starts_with("-s") { std::fs::read_to_string(&v).map_err(|e| format!("{v}: {e}"))? } else { v };
                    let mut host = Host::new(&mut scene, &mut history, &mut file);
                    let r = interp.run_no_undo(&mut host, &src);
                    print!("{}", interp.take_output());
                    let r = r?;
                    let s = r.as_string();
                    if !s.is_empty() {
                        println!("// Result: {s}");
                    }
                    Ok(())
                }
                "-render" | "-r" => {
                    let aspect = scene.render.width as f32 / scene.render.height.max(1) as f32;
                    let cam = rg_render::scene_camera(&scene, aspect).unwrap_or_else(|| {
                        let ev = scene.evaluate();
                        let b = scene.world_bounds(&scene.roots.clone(), &ev);
                        let c = if b.is_empty() { rg_core::glam::Vec3::ZERO } else { b.center() };
                        let r = if b.is_empty() { 5.0 } else { b.size().length().max(1.0) };
                        rg_render::RenderCamera::look_at(c + rg_core::glam::Vec3::new(1.0, 0.7, 1.2).normalize() * r * 1.3, c, 40.0)
                    });
                    let opts = rg_render::RenderOptions::from_scene(&scene);
                    let t = std::time::Instant::now();
                    let img = rg_render::render(&scene, &cam, &opts, None);
                    img.save_png(std::path::Path::new(&v))?;
                    println!("// Rendered {}x{} @ {} spp in {:.2}s -> {v}", opts.width, opts.height, opts.samples, t.elapsed().as_secs_f32());
                    Ok(())
                }
                "-save" => rg_io::export_file(&scene, std::path::Path::new(&v), None),
                other => Err(format!("unknown flag {other}")),
            }
        })();
        if let Err(e) = result {
            eprintln!("// Error: {e}");
            code = 1;
            break;
        }
    }
    std::process::exit(code);
}
