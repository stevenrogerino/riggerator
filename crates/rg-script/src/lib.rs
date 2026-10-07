//! rg-script — RigScript, a MEL-compatible command language.
//!
//! Supports MEL syntax (typed variables, arrays, vectors, procs, loops,
//! switch, backtick command substitution) and ~100 Maya commands with their
//! usual flags. Every command the UI performs is also available here, so
//! scenes can be built, rigged, animated and rendered from scripts.

pub mod builtins;
pub mod commands;
pub mod interp;
pub mod parse;
pub mod value;

pub use interp::{run_script, Host, HostRequest, Interpreter};
pub use value::Value;

#[cfg(test)]
mod tests {
    use super::*;
    use rg_core::{NodeKind, Scene};

    fn run(scene: &mut Scene, src: &str) -> (Value, String) {
        run_script(scene, src).unwrap_or_else(|e| panic!("script failed: {e}\n{src}"))
    }

    #[test]
    fn variables_loops_procs() {
        let mut s = Scene::new();
        let (_, out) = run(
            &mut s,
            r#"
            int $total = 0;
            for ($i = 1; $i <= 4; $i++) { $total += $i; }
            print ($total + "\n");
            proc float sq(float $x) { return $x * $x; }
            print (sq(3) + "\n");
            string $names[] = {"a", "b"};
            $names[2] = "c";
            print (size($names) + "\n");
            vector $v = <<1, 2, 3>>;
            print ($v + "\n");
            int $n = 0;
            while ($n < 10) { $n++; if ($n == 5) break; }
            print ($n + "\n");
            switch ($n) { case 5: print "five\n"; break; default: print "other\n"; }
            print `whatIs sq`;
            "#,
        );
        assert_eq!(out, "10\n9\n3\n1 2 3\n5\nfive\nMel procedure entered interactively.");
    }

    #[test]
    fn create_and_transform() {
        let mut s = Scene::new();
        let (v, _) = run(&mut s, "polyCube -w 2 -h 3 -n box; move -r 1 2 3; rotate 0 45 0; scale 2 2 2;");
        assert_eq!(v, Value::Str(String::new()));
        let id = s.find("box").unwrap();
        assert_eq!(s.get_attr(id, "tx").unwrap(), 1.0);
        assert_eq!(s.get_attr(id, "ty").unwrap(), 2.0);
        assert_eq!(s.get_attr(id, "ry").unwrap(), 45.0);
        assert_eq!(s.get_attr(id, "sx").unwrap(), 2.0);
        let (v, _) = run(&mut s, "getAttr box.translateZ;");
        assert_eq!(v.as_f64(), 3.0);
        let (v, _) = run(&mut s, "string $sel[] = `ls -sl`; $sel[0];");
        assert_eq!(v.as_string(), "box");
        let (v, _) = run(&mut s, "polyEvaluate -f box;");
        assert_eq!(v.as_i64(), 6);
    }

    #[test]
    fn modeling_commands() {
        let mut s = Scene::new();
        run(&mut s, "polyCube; polyExtrudeFacet -ltz 1 pCube1.f[1]; polySmooth -dv 1;");
        let id = s.find("pCube1").unwrap();
        let m = &s.nodes[&id].mesh().unwrap().mesh;
        assert!(m.validate().is_empty());
        assert_eq!(m.faces.len(), 40);
        run(&mut s, "polySphere -r 2 -n ball; polyUnite -n combo pCube1 ball; polySeparate combo;");
        assert!(s.find("combo").is_some());
    }

    #[test]
    fn animation_and_keys() {
        let mut s = Scene::new();
        run(&mut s, "polySphere -n ball; setKeyframe -at ty -t 1 -v 0; setKeyframe -at ty -t 11 -v 10; currentTime 6;");
        let id = s.find("ball").unwrap();
        let ty = s.get_attr(id, "ty").unwrap();
        assert!(ty > 2.0 && ty < 8.0);
        let (v, _) = run(&mut s, "keyframe -q -kc ball;");
        assert_eq!(v.as_i64(), 2);
        run(&mut s, "keyTangent -itt linear -ott linear ball; currentTime 6;");
        assert!((s.get_attr(id, "ty").unwrap() - 5.0).abs() < 1e-6);
        run(&mut s, "setInfinity -poi cycle -at ty ball; currentTime 16;");
        assert!((s.get_attr(id, "ty").unwrap() - 5.0).abs() < 1e-6);
        run(&mut s, "cutKey -cl -at ty ball;");
        assert!(s.nodes[&id].anim.is_empty());
    }

    #[test]
    fn rigging_commands() {
        let mut s = Scene::new();
        run(
            &mut s,
            r#"
            polyCylinder -r 0.3 -h 4 -sh 6 -n arm; move 0 2 0;
            select -cl;
            joint -p 0 0 0; joint -p 0 2 0.2; joint -p 0 4 0;
            ikHandle -sj joint1 -ee joint3 -n armIK;
            skinCluster joint1 arm;
            spaceLocator -n target; move 1 3 0;
            pointConstraint target armIK;
            circle -nr 0 1 0 -r 2 -n ctrl;
            "#,
        );
        let ev = s.evaluate();
        let end = s.find("joint3").unwrap();
        let p = ev.world(end).w_axis.truncate();
        assert!((p - glam::Vec3::new(1.0, 3.0, 0.0)).length() < 1e-2, "{p:?}");
        let arm = s.find("arm").unwrap();
        assert!(ev.is_deformed(arm));
        assert!(matches!(s.nodes[&s.find("ctrl").unwrap()].kind, NodeKind::Curve(_)));
    }

    #[test]
    fn driven_keys_and_attrs() {
        let mut s = Scene::new();
        run(
            &mut s,
            r#"
            spaceLocator -n drv; polyCube -n box;
            addAttr -ln "flex" -dv 0 drv;
            setDrivenKeyframe -cd drv.flex -dv 0 -v 0 box.ry;
            setDrivenKeyframe -cd drv.flex -dv 10 -v 90 box.ry;
            setAttr drv.flex 10;
            "#,
        );
        let ev = s.evaluate();
        let b = s.find("box").unwrap();
        assert!((ev.attr(&s, b, "ry").unwrap() - 90.0).abs() < 1e-4);
    }

    #[test]
    fn materials_and_files() {
        let dir = std::env::temp_dir().join(format!("rgscript_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.rgs").display().to_string().replace('\\', "/");
        let mut s = Scene::new();
        run(
            &mut s,
            &format!(
                r#"
                polySphere -n ball;
                string $m = `shadingNode -asShader blinn -n red`;
                setAttr ($m + ".color") -type double3 1 0 0;
                hyperShade -assign $m;
                file -rn "{p}"; file -s;
                file -new -f;
                "#
            ),
        );
        assert!(s.nodes.is_empty());
        run(&mut s, &format!("file -o \"{p}\";"));
        let id = s.find("ball").unwrap();
        assert!((s.material_for(id).color.x - 1.0).abs() < 1e-6);
    }

    #[test]
    fn undo_chunks() {
        let mut s = Scene::new();
        let mut h = rg_core::History::new();
        let mut f = None;
        let mut interp = Interpreter::new();
        {
            let mut host = Host::new(&mut s, &mut h, &mut f);
            interp.run(&mut host, "polyCube; polyCube;", "cubes").unwrap();
        }
        assert_eq!(s.nodes.len(), 2);
        {
            let mut host = Host::new(&mut s, &mut h, &mut f);
            interp.run(&mut host, "undo;", "undo").unwrap();
        }
        assert_eq!(s.nodes.len(), 0);
    }

    #[test]
    fn errors_are_reported() {
        let mut s = Scene::new();
        assert!(run_script(&mut s, "fooBar 1 2;").unwrap_err().contains("Cannot find procedure"));
        assert!(run_script(&mut s, "select nothingHere;").is_err());
        let (v, out) = run(&mut s, "catch(`select nope`);");
        assert_eq!(v.as_i64(), 1);
        assert!(out.contains("Error"));
    }
}
