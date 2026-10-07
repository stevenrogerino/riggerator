use crate::commands;
use crate::parse::{parse, CmdArg, CmdCall, Expr, Proc, Stmt};
use crate::value::Value;
use rg_core::{History, Scene};
use std::collections::HashMap;
use std::path::PathBuf;

/// Requests a script makes of the host application (UI side effects).
#[derive(Clone, Debug, PartialEq)]
pub enum HostRequest {
    FrameSelected,
    FrameAll,
    OpenWindow(String),
    RenderView,
    Playback(bool),
    Message(String),
}

pub struct Host<'a> {
    pub scene: &'a mut Scene,
    pub history: &'a mut History,
    pub file: &'a mut Option<PathBuf>,
    pub requests: Vec<HostRequest>,
    /// Set when the script performed undo/redo itself.
    pub touched_history: bool,
}

impl<'a> Host<'a> {
    pub fn new(scene: &'a mut Scene, history: &'a mut History, file: &'a mut Option<PathBuf>) -> Self {
        Self { scene, history, file, requests: Vec::new(), touched_history: false }
    }
}

enum Flow {
    Normal,
    Break,
    Continue,
    Return(Value),
}

#[derive(Default)]
pub struct Interpreter {
    globals: HashMap<String, Value>,
    scopes: Vec<HashMap<String, Value>>,
    global_names: Vec<std::collections::HashSet<String>>,
    procs: HashMap<String, Proc>,
    pub output: Vec<String>,
    depth: usize,
    rng: u64,
}

impl Interpreter {
    pub fn new() -> Self {
        Self { rng: 0x9E3779B97F4A7C15, ..Default::default() }
    }

    pub fn print(&mut self, s: &str) {
        self.output.push(s.to_string());
    }

    pub fn take_output(&mut self) -> String {
        let s = self.output.concat();
        self.output.clear();
        s
    }

    pub fn proc_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.procs.keys().cloned().collect();
        v.sort();
        v
    }

    pub fn rand(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Run source as one undoable chunk. Returns the last value produced.
    pub fn run(&mut self, host: &mut Host, src: &str, label: &str) -> Result<Value, String> {
        let prog = parse(src)?;
        let before = host.scene.clone();
        let rev = host.scene.revision;
        let res = self.exec_top(host, &prog);
        if !host.touched_history && host.scene.revision != rev {
            host.history.push(label, &before);
        }
        res
    }

    /// Run without recording undo (used by batch tools and tests).
    pub fn run_no_undo(&mut self, host: &mut Host, src: &str) -> Result<Value, String> {
        let prog = parse(src)?;
        self.exec_top(host, &prog)
    }

    fn exec_top(&mut self, host: &mut Host, prog: &[Stmt]) -> Result<Value, String> {
        if self.scopes.is_empty() {
            self.scopes.push(HashMap::new());
            self.global_names.push(Default::default());
        }
        let mut last = Value::Str(String::new());
        for st in prog {
            if let Stmt::Expr(e, _) = st {
                last = self.eval(host, e)?;
                continue;
            }
            match self.exec(host, st)? {
                Flow::Return(v) => return Ok(v),
                _ => last = Value::Str(String::new()),
            }
        }
        Ok(last)
    }

    fn lookup(&self, name: &str) -> Option<&Value> {
        if let Some(g) = self.global_names.last() {
            if g.contains(name) {
                return self.globals.get(name);
            }
        }
        if let Some(v) = self.scopes.last().and_then(|s| s.get(name)) {
            return Some(v);
        }
        if self.scopes.len() == 1 {
            return self.globals.get(name);
        }
        None
    }

    fn store(&mut self, name: &str, v: Value) {
        if self.global_names.last().is_some_and(|g| g.contains(name)) || (self.scopes.len() == 1 && self.globals.contains_key(name)) {
            self.globals.insert(name.to_string(), v);
            return;
        }
        if let Some(s) = self.scopes.last_mut() {
            s.insert(name.to_string(), v);
        }
    }

    fn exec_block(&mut self, host: &mut Host, stmts: &[Stmt]) -> Result<Flow, String> {
        for s in stmts {
            match self.exec(host, s)? {
                Flow::Normal => {}
                f => return Ok(f),
            }
        }
        Ok(Flow::Normal)
    }

    fn exec(&mut self, host: &mut Host, st: &Stmt) -> Result<Flow, String> {
        match st {
            Stmt::Empty => Ok(Flow::Normal),
            Stmt::Expr(e, _) => {
                self.eval(host, e)?;
                Ok(Flow::Normal)
            }
            Stmt::Decl { ty, name, array, init, global } => {
                if *global {
                    if let Some(g) = self.global_names.last_mut() {
                        g.insert(name.clone());
                    }
                    if self.globals.contains_key(name) && init.is_none() {
                        return Ok(Flow::Normal);
                    }
                }
                let v = match init {
                    Some(e) => self.eval(host, e)?,
                    None => {
                        if *array {
                            Value::Array(Vec::new())
                        } else {
                            match ty.as_str() {
                                "int" => Value::Int(0),
                                "float" => Value::Float(0.0),
                                "vector" => Value::Vector([0.0; 3]),
                                _ => Value::Str(String::new()),
                            }
                        }
                    }
                };
                let v = if *array {
                    match v {
                        Value::Array(a) => Value::Array(a.into_iter().map(|x| x.coerce(ty)).collect()),
                        other => Value::Array(vec![other.coerce(ty)]),
                    }
                } else {
                    v.coerce(ty)
                };
                if *global {
                    self.globals.insert(name.clone(), v);
                } else {
                    self.store(name, v);
                }
                Ok(Flow::Normal)
            }
            Stmt::If(c, a, b) => {
                if self.eval(host, c)?.truthy() {
                    self.exec(host, a)
                } else if let Some(b) = b {
                    self.exec(host, b)
                } else {
                    Ok(Flow::Normal)
                }
            }
            Stmt::While(c, body) => {
                let mut guard = 0u64;
                while self.eval(host, c)?.truthy() {
                    guard += 1;
                    if guard > 10_000_000 {
                        return Err("while loop exceeded 10 million iterations".into());
                    }
                    match self.exec(host, body)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        _ => {}
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::DoWhile(body, c) => {
                loop {
                    match self.exec(host, body)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        _ => {}
                    }
                    if !self.eval(host, c)?.truthy() {
                        break;
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::For(init, cond, step, body) => {
                if let Some(i) = init {
                    self.exec(host, i)?;
                }
                let mut guard = 0u64;
                loop {
                    if let Some(c) = cond {
                        if !self.eval(host, c)?.truthy() {
                            break;
                        }
                    }
                    guard += 1;
                    if guard > 10_000_000 {
                        return Err("for loop exceeded 10 million iterations".into());
                    }
                    match self.exec(host, body)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        _ => {}
                    }
                    if let Some(s) = step {
                        self.eval(host, s)?;
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::ForIn(var, e, body) => {
                let items = self.eval(host, e)?.flatten();
                for it in items {
                    self.store(var, it);
                    match self.exec(host, body)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        _ => {}
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::Block(b) => self.exec_block(host, b),
            Stmt::Proc(p) => {
                self.procs.insert(p.name.clone(), p.clone());
                Ok(Flow::Normal)
            }
            Stmt::Return(e) => {
                let v = match e {
                    Some(e) => self.eval(host, e)?,
                    None => Value::Str(String::new()),
                };
                Ok(Flow::Return(v))
            }
            Stmt::Break => Ok(Flow::Break),
            Stmt::Continue => Ok(Flow::Continue),
            Stmt::Switch(e, cases) => {
                let v = self.eval(host, e)?;
                let mut matched = false;
                for (label, body) in cases {
                    if !matched {
                        matched = match label {
                            None => true,
                            Some(l) => {
                                let lv = self.eval(host, l)?;
                                values_equal(&v, &lv)
                            }
                        };
                    }
                    if matched {
                        match self.exec_block(host, body)? {
                            Flow::Break => return Ok(Flow::Normal),
                            Flow::Normal => {}
                            f => return Ok(f),
                        }
                    }
                }
                Ok(Flow::Normal)
            }
        }
    }

    pub fn eval(&mut self, host: &mut Host, e: &Expr) -> Result<Value, String> {
        Ok(match e {
            Expr::Num(n, is_int) => {
                if *is_int {
                    Value::Int(*n as i64)
                } else {
                    Value::Float(*n)
                }
            }
            Expr::Str(s) => Value::Str(s.clone()),
            Expr::Var(name, idx, _) => {
                let v = self.lookup(name).cloned().ok_or_else(|| format!("variable ${name} is undefined"))?;
                match idx {
                    Some(i) => {
                        let i = self.eval(host, i)?.as_i64();
                        match v {
                            Value::Array(a) => a.get(i.max(0) as usize).cloned().unwrap_or_default(),
                            Value::Vector(vv) => Value::Float(*vv.get(i.max(0) as usize).unwrap_or(&0.0)),
                            other => other,
                        }
                    }
                    None => v,
                }
            }
            Expr::Vector(parts) => {
                let a = self.eval(host, &parts[0])?.as_f64();
                let b = self.eval(host, &parts[1])?.as_f64();
                let c = self.eval(host, &parts[2])?.as_f64();
                Value::Vector([a, b, c])
            }
            Expr::Array(items) => {
                let mut v = Vec::new();
                for it in items {
                    v.push(self.eval(host, it)?);
                }
                Value::Array(v)
            }
            Expr::Unary(op, a) => {
                let v = self.eval(host, a)?;
                match *op {
                    "-" => match v {
                        Value::Int(i) => Value::Int(-i),
                        Value::Vector(x) => Value::Vector([-x[0], -x[1], -x[2]]),
                        Value::Str(s) => Value::Str(format!("-{s}")),
                        o => Value::Float(-o.as_f64()),
                    },
                    _ => Value::Int(!v.truthy() as i64),
                }
            }
            Expr::Binary(op, a, b) => {
                if *op == "&&" {
                    return Ok(Value::Int((self.eval(host, a)?.truthy() && self.eval(host, b)?.truthy()) as i64));
                }
                if *op == "||" {
                    return Ok(Value::Int((self.eval(host, a)?.truthy() || self.eval(host, b)?.truthy()) as i64));
                }
                let x = self.eval(host, a)?;
                let y = self.eval(host, b)?;
                binop(op, &x, &y)?
            }
            Expr::Ternary(c, a, b) => {
                if self.eval(host, c)?.truthy() {
                    self.eval(host, a)?
                } else {
                    self.eval(host, b)?
                }
            }
            Expr::Assign(name, idx, op, rhs) => {
                let r = self.eval(host, rhs)?;
                let cur = self.lookup(name).cloned();
                let newv = match idx {
                    Some(i) => {
                        let i = self.eval(host, i)?.as_i64().max(0) as usize;
                        let mut arr = match cur {
                            Some(Value::Array(a)) => a,
                            Some(Value::Vector(v)) => {
                                let mut v = v;
                                if i < 3 {
                                    let old = Value::Float(v[i]);
                                    v[i] = apply_op(op, Some(old), r)?.as_f64();
                                }
                                self.store(name, Value::Vector(v));
                                return Ok(Value::Vector(v));
                            }
                            _ => Vec::new(),
                        };
                        if arr.len() <= i {
                            let fill = arr.first().map(|v| match v {
                                Value::Int(_) => Value::Int(0),
                                Value::Float(_) => Value::Float(0.0),
                                _ => Value::Str(String::new()),
                            });
                            arr.resize(i + 1, fill.unwrap_or_default());
                        }
                        let old = arr[i].clone();
                        let nv = apply_op(op, Some(old.clone()), r)?;
                        // keep element type
                        arr[i] = if old.is_numeric() && nv.is_numeric() { nv.coerce(old.type_name()) } else { nv };
                        Value::Array(arr)
                    }
                    None => {
                        let nv = apply_op(op, cur.clone(), r)?;
                        match cur {
                            Some(c) if !matches!(c, Value::Str(ref s) if s.is_empty()) && c.type_name() != "array" => nv.coerce(c.type_name()),
                            _ => nv,
                        }
                    }
                };
                self.store(name, newv.clone());
                newv
            }
            Expr::IncDec(name, d, prefix) => {
                let cur = self.lookup(name).cloned().ok_or_else(|| format!("variable ${name} is undefined"))?;
                let nv = match &cur {
                    Value::Int(i) => Value::Int(i + *d as i64),
                    o => Value::Float(o.as_f64() + d),
                };
                self.store(name, nv.clone());
                if *prefix { nv } else { cur }
            }
            Expr::Call(name, args) if name == "catch" || name == "catchQuiet" => {
                let mut failed = false;
                for a in args {
                    if let Err(e) = self.eval(host, a) {
                        if name == "catch" {
                            self.print(&format!("// Error: {e}\n"));
                        }
                        failed = true;
                    }
                }
                Value::Int(failed as i64)
            }
            Expr::Call(name, args) => {
                let mut vals = Vec::new();
                for a in args {
                    vals.push(self.eval(host, a)?);
                }
                self.call(host, name, vals)?
            }
            Expr::Command(c) => self.command(host, c)?,
        })
    }

    pub fn call(&mut self, host: &mut Host, name: &str, args: Vec<Value>) -> Result<Value, String> {
        if let Some(p) = self.procs.get(name).cloned() {
            return self.call_proc(host, &p, args);
        }
        if let Some(v) = crate::builtins::function(self, host, name, &args)? {
            return Ok(v);
        }
        // command in function form: string args starting with '-' are flags
        let mut cargs = Vec::new();
        for a in args {
            match &a {
                Value::Str(s) if s.starts_with('-') && s[1..].chars().next().is_some_and(|c| c.is_alphabetic()) => {
                    cargs.push(commands::RawArg::Flag(s[1..].to_string()))
                }
                _ => cargs.push(commands::RawArg::Val(a)),
            }
        }
        commands::dispatch(self, host, name, cargs)
    }

    fn call_proc(&mut self, host: &mut Host, p: &Proc, args: Vec<Value>) -> Result<Value, String> {
        if self.depth > 200 {
            return Err(format!("{}: recursion too deep", p.name));
        }
        let mut scope = HashMap::new();
        for (i, (ty, name, arr)) in p.params.iter().enumerate() {
            let v = args.get(i).cloned().unwrap_or_default();
            scope.insert(name.clone(), if *arr { v } else { v.coerce(ty) });
        }
        self.scopes.push(scope);
        self.global_names.push(Default::default());
        self.depth += 1;
        let r = self.exec_block(host, &p.body);
        self.depth -= 1;
        self.scopes.pop();
        self.global_names.pop();
        match r? {
            Flow::Return(v) => Ok(v),
            _ => Ok(Value::Str(String::new())),
        }
    }

    fn command(&mut self, host: &mut Host, c: &CmdCall) -> Result<Value, String> {
        if let Some(p) = self.procs.get(&c.name).cloned() {
            let mut vals = Vec::new();
            for a in &c.args {
                if let CmdArg::Val(e) = a {
                    vals.push(self.eval(host, e)?);
                }
            }
            return self.call_proc(host, &p, vals);
        }
        let mut raw = Vec::new();
        for a in &c.args {
            match a {
                CmdArg::Flag(f) => raw.push(commands::RawArg::Flag(f.clone())),
                CmdArg::Val(e) => raw.push(commands::RawArg::Val(self.eval(host, e)?)),
            }
        }
        if let Some(v) = crate::builtins::command_form(self, host, &c.name, &raw)? {
            return Ok(v);
        }
        commands::dispatch(self, host, &c.name, raw).map_err(|e| format!("line {}: {}: {e}", c.line, c.name))
    }
}

pub fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Str(_), _) | (_, Value::Str(_)) => a.as_string() == b.as_string(),
        _ => (a.as_f64() - b.as_f64()).abs() < 1e-12,
    }
}

fn apply_op(op: &str, cur: Option<Value>, r: Value) -> Result<Value, String> {
    match op {
        "=" => Ok(r),
        "+=" => binop("+", &cur.unwrap_or(Value::Int(0)), &r),
        "-=" => binop("-", &cur.unwrap_or(Value::Int(0)), &r),
        "*=" => binop("*", &cur.unwrap_or(Value::Int(0)), &r),
        "/=" => binop("/", &cur.unwrap_or(Value::Int(0)), &r),
        _ => Err(format!("bad assignment operator {op}")),
    }
}

pub fn binop(op: &str, x: &Value, y: &Value) -> Result<Value, String> {
    use Value::*;
    // string concatenation
    if op == "+" && (matches!(x, Str(_)) || matches!(y, Str(_))) {
        return Ok(Str(format!("{}{}", x.as_string(), y.as_string())));
    }
    if let (Array(a), Array(b)) = (x, y) {
        if op == "+" {
            let mut v = a.clone();
            v.extend(b.iter().cloned());
            return Ok(Array(v));
        }
    }
    let cmp = |o: &str| -> Option<bool> {
        let (a, b) = (x.as_f64(), y.as_f64());
        Some(match o {
            "==" => return Some(values_equal(x, y)),
            "!=" => return Some(!values_equal(x, y)),
            "<" => a < b,
            ">" => a > b,
            "<=" => a <= b,
            ">=" => a >= b,
            _ => return None,
        })
    };
    if let Some(b) = cmp(op) {
        return Ok(Int(b as i64));
    }
    if let (Vector(a), _) | (_, Vector(a)) = (x, y) {
        let _ = a;
        let av = x.as_vec3();
        let bv = y.as_vec3();
        let r = match op {
            "+" => [av[0] + bv[0], av[1] + bv[1], av[2] + bv[2]],
            "-" => [av[0] - bv[0], av[1] - bv[1], av[2] - bv[2]],
            "*" => {
                if matches!(x, Vector(_)) && matches!(y, Vector(_)) {
                    return Ok(Float(av[0] * bv[0] + av[1] * bv[1] + av[2] * bv[2]));
                }
                [av[0] * bv[0], av[1] * bv[1], av[2] * bv[2]]
            }
            "/" => [av[0] / bv[0], av[1] / bv[1], av[2] / bv[2]],
            _ => return Err(format!("operator {op} not supported on vectors")),
        };
        return Ok(Vector(r));
    }
    if let (Int(a), Int(b)) = (x, y) {
        return Ok(match op {
            "+" => Int(a.wrapping_add(*b)),
            "-" => Int(a.wrapping_sub(*b)),
            "*" => Int(a.wrapping_mul(*b)),
            "/" => {
                if *b == 0 {
                    return Err("integer division by zero".into());
                }
                Int(a / b)
            }
            "%" => {
                if *b == 0 {
                    return Err("modulo by zero".into());
                }
                Int(a % b)
            }
            _ => return Err(format!("unknown operator {op}")),
        });
    }
    let (a, b) = (x.as_f64(), y.as_f64());
    Ok(Float(match op {
        "+" => a + b,
        "-" => a - b,
        "*" => a * b,
        "/" => a / b,
        "%" => a % b,
        _ => return Err(format!("unknown operator {op}")),
    }))
}

pub fn run_script(scene: &mut Scene, src: &str) -> Result<(Value, String), String> {
    let mut h = History::new();
    let mut f = None;
    let mut host = Host::new(scene, &mut h, &mut f);
    let mut i = Interpreter::new();
    let v = i.run_no_undo(&mut host, src)?;
    Ok((v, i.take_output()))
}
