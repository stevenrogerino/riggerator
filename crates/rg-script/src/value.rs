#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    Vector([f64; 3]),
    Array(Vec<Value>),
}

impl Default for Value {
    fn default() -> Self {
        Value::Str(String::new())
    }
}

pub fn fmt_float(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 1e15 {
        return format!("{}", f as i64);
    }
    let s = format!("{f:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

impl Value {
    pub fn as_f64(&self) -> f64 {
        match self {
            Value::Int(i) => *i as f64,
            Value::Float(f) => *f,
            Value::Str(s) => s.trim().parse().unwrap_or(0.0),
            Value::Vector(v) => v[0],
            Value::Array(a) => a.first().map(|v| v.as_f64()).unwrap_or(0.0),
        }
    }
    pub fn as_i64(&self) -> i64 {
        match self {
            Value::Int(i) => *i,
            other => other.as_f64() as i64,
        }
    }
    pub fn truthy(&self) -> bool {
        match self {
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => !s.is_empty() && s != "0" && s != "false" && s != "off" && s != "no",
            Value::Vector(v) => v.iter().any(|x| *x != 0.0),
            Value::Array(a) => !a.is_empty(),
        }
    }
    pub fn as_string(&self) -> String {
        match self {
            Value::Int(i) => i.to_string(),
            Value::Float(f) => fmt_float(*f),
            Value::Str(s) => s.clone(),
            Value::Vector(v) => format!("{} {} {}", fmt_float(v[0]), fmt_float(v[1]), fmt_float(v[2])),
            Value::Array(a) => a.iter().map(|v| v.as_string()).collect::<Vec<_>>().join(" "),
        }
    }
    /// Text used by `print` (arrays print one element per line, like MEL).
    pub fn print_string(&self) -> String {
        match self {
            Value::Array(a) => a.iter().map(|v| format!("{}\n", v.as_string())).collect(),
            other => other.as_string(),
        }
    }
    pub fn as_vec3(&self) -> [f64; 3] {
        match self {
            Value::Vector(v) => *v,
            Value::Array(a) if a.len() >= 3 => [a[0].as_f64(), a[1].as_f64(), a[2].as_f64()],
            Value::Str(s) => {
                let n: Vec<f64> = s.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                [n.first().copied().unwrap_or(0.0), n.get(1).copied().unwrap_or(0.0), n.get(2).copied().unwrap_or(0.0)]
            }
            other => {
                let f = other.as_f64();
                [f, f, f]
            }
        }
    }
    pub fn flatten(self) -> Vec<Value> {
        match self {
            Value::Array(a) => a.into_iter().flat_map(|v| v.flatten()).collect(),
            v => vec![v],
        }
    }
    pub fn strings(v: Vec<String>) -> Value {
        Value::Array(v.into_iter().map(Value::Str).collect())
    }
    pub fn is_numeric(&self) -> bool {
        matches!(self, Value::Int(_) | Value::Float(_))
    }
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Str(_) => "string",
            Value::Vector(_) => "vector",
            Value::Array(_) => "array",
        }
    }
    /// Coerce to a declared MEL type.
    pub fn coerce(self, ty: &str) -> Value {
        match ty {
            "int" => match self {
                Value::Array(a) => Value::Array(a),
                v => Value::Int(v.as_f64() as i64),
            },
            "float" => match self {
                Value::Array(a) => Value::Array(a),
                v => Value::Float(v.as_f64()),
            },
            "string" => match self {
                Value::Array(a) => Value::Array(a),
                v => Value::Str(v.as_string()),
            },
            "vector" => match self {
                Value::Array(a) if a.len() != 3 => Value::Array(a),
                v => Value::Vector(v.as_vec3()),
            },
            _ => self,
        }
    }
}
