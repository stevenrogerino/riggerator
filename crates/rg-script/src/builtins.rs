//! Pure functions (math, strings, arrays) and interpreter-level commands.

use crate::commands::RawArg;
use crate::interp::{Host, Interpreter};
use crate::value::Value;

fn f(args: &[Value], i: usize) -> f64 {
    args.get(i).map(|v| v.as_f64()).unwrap_or(0.0)
}
fn s(args: &[Value], i: usize) -> String {
    args.get(i).map(|v| v.as_string()).unwrap_or_default()
}

pub const FUNCTIONS: &[&str] = &[
    "abs", "sign", "sin", "cos", "tan", "asin", "acos", "atan", "atan2", "sqrt", "pow", "exp", "log", "log10", "floor", "ceil",
    "trunc", "round", "rand", "seed", "min", "max", "clamp", "deg_to_rad", "rad_to_deg", "linstep", "smoothstep", "size", "strcmp",
    "substring", "tolower", "toupper", "startsWith", "endsWith", "gmatch", "substitute", "stringToStringArray",
    "stringArrayToString", "stringArrayContains", "sort", "mag", "unit", "cross", "dot", "angle", "eval", "catch", "catchQuiet",
    "print", "strip", "sizeBytes", "floatEq", "isNumeric", "lerp", "noise", "stringArrayRemoveDuplicates",
];

pub fn function(interp: &mut Interpreter, host: &mut Host, name: &str, args: &[Value]) -> Result<Option<Value>, String> {
    let v = match name {
        "abs" => match args.first() {
            Some(Value::Int(i)) => Value::Int(i.abs()),
            Some(Value::Vector(v)) => Value::Vector([v[0].abs(), v[1].abs(), v[2].abs()]),
            _ => Value::Float(f(args, 0).abs()),
        },
        "sign" => Value::Float(f(args, 0).signum()),
        "sin" => Value::Float(f(args, 0).sin()),
        "cos" => Value::Float(f(args, 0).cos()),
        "tan" => Value::Float(f(args, 0).tan()),
        "asin" => Value::Float(f(args, 0).asin()),
        "acos" => Value::Float(f(args, 0).acos()),
        "atan" => Value::Float(f(args, 0).atan()),
        "atan2" => Value::Float(f(args, 0).atan2(f(args, 1))),
        "sqrt" => Value::Float(f(args, 0).sqrt()),
        "pow" => Value::Float(f(args, 0).powf(f(args, 1))),
        "exp" => Value::Float(f(args, 0).exp()),
        "log" => Value::Float(f(args, 0).ln()),
        "log10" => Value::Float(f(args, 0).log10()),
        "floor" => Value::Float(f(args, 0).floor()),
        "ceil" => Value::Float(f(args, 0).ceil()),
        "trunc" => Value::Float(f(args, 0).trunc()),
        "round" => Value::Float(f(args, 0).round()),
        "rand" => {
            let r = interp.rand();
            match args.len() {
                0 => Value::Float(r),
                1 => Value::Float(r * f(args, 0)),
                _ => Value::Float(f(args, 0) + r * (f(args, 1) - f(args, 0))),
            }
        }
        "seed" => Value::Int(0),
        "min" => Value::Float(f(args, 0).min(f(args, 1))),
        "max" => Value::Float(f(args, 0).max(f(args, 1))),
        "clamp" => Value::Float(f(args, 2).clamp(f(args, 0).min(f(args, 1)), f(args, 1).max(f(args, 0)))),
        "lerp" => Value::Float(f(args, 0) + (f(args, 1) - f(args, 0)) * f(args, 2)),
        "deg_to_rad" => Value::Float(f(args, 0).to_radians()),
        "rad_to_deg" => Value::Float(f(args, 0).to_degrees()),
        "linstep" => {
            let (a, b, x) = (f(args, 0), f(args, 1), f(args, 2));
            Value::Float(((x - a) / (b - a)).clamp(0.0, 1.0))
        }
        "smoothstep" => {
            let (a, b, x) = (f(args, 0), f(args, 1), f(args, 2));
            let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
            Value::Float(t * t * (3.0 - 2.0 * t))
        }
        "noise" => {
            let x = f(args, 0);
            Value::Float(((x * 12.9898).sin() * 43758.5453).fract() * 2.0 - 1.0)
        }
        "size" => match args.first() {
            Some(Value::Array(a)) => Value::Int(a.len() as i64),
            Some(v) => Value::Int(v.as_string().chars().count() as i64),
            None => Value::Int(0),
        },
        "sizeBytes" => Value::Int(s(args, 0).len() as i64),
        "strcmp" => Value::Int(match s(args, 0).cmp(&s(args, 1)) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        }),
        "substring" => {
            let st = s(args, 0);
            let chars: Vec<char> = st.chars().collect();
            let a = (f(args, 1) as usize).max(1) - 1;
            let b = (f(args, 2) as usize).min(chars.len());
            Value::Str(if a < b { chars[a..b].iter().collect() } else { String::new() })
        }
        "tolower" => Value::Str(s(args, 0).to_lowercase()),
        "toupper" => Value::Str(s(args, 0).to_uppercase()),
        "strip" => Value::Str(s(args, 0).trim().to_string()),
        "startsWith" => Value::Int(s(args, 0).starts_with(&s(args, 1)) as i64),
        "endsWith" => Value::Int(s(args, 0).ends_with(&s(args, 1)) as i64),
        "gmatch" => Value::Int(crate::commands::glob_match(&s(args, 1), &s(args, 0)) as i64),
        "substitute" => Value::Str(s(args, 1).replacen(&s(args, 0), &s(args, 2), 1)),
        "isNumeric" => Value::Int(s(args, 0).trim().parse::<f64>().is_ok() as i64),
        "floatEq" => Value::Int(((f(args, 0) - f(args, 1)).abs() <= args.get(2).map(|v| v.as_f64()).unwrap_or(1e-5)) as i64),
        "stringToStringArray" => {
            let sep = s(args, 1);
            let st = s(args, 0);
            let parts: Vec<String> = if sep.is_empty() {
                st.split_whitespace().map(String::from).collect()
            } else {
                st.split(|c| sep.contains(c)).filter(|p| !p.is_empty()).map(String::from).collect()
            };
            Value::strings(parts)
        }
        "stringArrayToString" => {
            let parts: Vec<String> = args.first().cloned().unwrap_or_default().flatten().iter().map(|v| v.as_string()).collect();
            Value::Str(parts.join(&s(args, 1)))
        }
        "stringArrayContains" => {
            let needle = s(args, 0);
            Value::Int(args.get(1).cloned().unwrap_or_default().flatten().iter().any(|v| v.as_string() == needle) as i64)
        }
        "stringArrayRemoveDuplicates" => {
            let mut seen = Vec::new();
            for v in args.first().cloned().unwrap_or_default().flatten() {
                if !seen.contains(&v) {
                    seen.push(v);
                }
            }
            Value::Array(seen)
        }
        "sort" => {
            let mut v = args.first().cloned().unwrap_or_default().flatten();
            v.sort_by(|a, b| {
                if a.is_numeric() && b.is_numeric() {
                    a.as_f64().total_cmp(&b.as_f64())
                } else {
                    a.as_string().cmp(&b.as_string())
                }
            });
            Value::Array(v)
        }
        "mag" => {
            let v = args.first().cloned().unwrap_or_default().as_vec3();
            Value::Float((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt())
        }
        "unit" => {
            let v = args.first().cloned().unwrap_or_default().as_vec3();
            let m = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-12);
            Value::Vector([v[0] / m, v[1] / m, v[2] / m])
        }
        "cross" => {
            let a = args.first().cloned().unwrap_or_default().as_vec3();
            let b = args.get(1).cloned().unwrap_or_default().as_vec3();
            Value::Vector([a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]])
        }
        "dot" => {
            let a = args.first().cloned().unwrap_or_default().as_vec3();
            let b = args.get(1).cloned().unwrap_or_default().as_vec3();
            Value::Float(a[0] * b[0] + a[1] * b[1] + a[2] * b[2])
        }
        "angle" => {
            let a = glam::DVec3::from(args.first().cloned().unwrap_or_default().as_vec3());
            let b = glam::DVec3::from(args.get(1).cloned().unwrap_or_default().as_vec3());
            Value::Float(a.angle_between(b))
        }
        "eval" => {
            let src = args.iter().map(|a| a.as_string()).collect::<Vec<_>>().join(" ");
            let prog = crate::parse::parse(&src)?;
            let mut last = Value::default();
            for st in &prog {
                if let crate::parse::Stmt::Expr(e, _) = st {
                    last = interp.eval(host, e)?;
                } else {
                    // reuse the main executor through run_no_undo for statements
                    return interp.run_no_undo(host, &src).map(Some);
                }
            }
            last
        }
        "print" => {
            let t: String = args.iter().map(|a| a.print_string()).collect();
            interp.print(&t);
            Value::default()
        }
        _ => return Ok(None),
    };
    Ok(Some(v))
}

/// Commands handled by the interpreter rather than the scene.
pub fn command_form(interp: &mut Interpreter, host: &mut Host, name: &str, raw: &[RawArg]) -> Result<Option<Value>, String> {
    let vals: Vec<Value> = raw
        .iter()
        .filter_map(|a| match a {
            RawArg::Val(v) => Some(v.clone()),
            RawArg::Flag(_) => None,
        })
        .collect();
    match name {
        "print" => {
            let t: String = vals.iter().map(|a| a.print_string()).collect();
            interp.print(&t);
            Ok(Some(Value::default()))
        }
        "eval" => {
            let src = vals.iter().map(|a| a.as_string()).collect::<Vec<_>>().join(" ");
            interp.run_no_undo(host, &src).map(Some)
        }
        "source" => {
            let path = vals.first().map(|v| v.as_string()).unwrap_or_default();
            let src = std::fs::read_to_string(&path).map_err(|e| format!("source {path}: {e}"))?;
            interp.run_no_undo(host, &src).map(Some)
        }
        "whatIs" => {
            let n = vals.first().map(|v| v.as_string()).unwrap_or_default();
            let r = if interp.proc_names().contains(&n) {
                "Mel procedure entered interactively."
            } else if FUNCTIONS.contains(&n.as_str()) {
                "Function"
            } else if crate::commands::COMMANDS.iter().any(|(c, _)| *c == n) {
                "Command"
            } else {
                "Unknown"
            };
            Ok(Some(Value::Str(r.into())))
        }
        "help" => {
            let topic = vals.first().map(|v| v.as_string());
            match topic {
                Some(t) => {
                    let line = crate::commands::COMMANDS
                        .iter()
                        .find(|(c, _)| *c == t)
                        .map(|(c, h)| format!("{c}: {h}\n"))
                        .unwrap_or_else(|| format!("No help for '{t}'.\n"));
                    interp.print(&line);
                }
                None => {
                    let mut s = String::from("Commands:\n");
                    for (c, h) in crate::commands::COMMANDS {
                        s.push_str(&format!("  {c:<22} {h}\n"));
                    }
                    s.push_str("Functions: ");
                    s.push_str(&FUNCTIONS.join(", "));
                    s.push('\n');
                    interp.print(&s);
                }
            }
            Ok(Some(Value::default()))
        }
        _ => Ok(None),
    }
}
