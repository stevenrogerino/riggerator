//! Lexer and parser for RigScript, a MEL-compatible scripting language.

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Word(String),
    Var(String),
    Num(f64, bool), // value, is_int
    Str(String),
    Flag(String),
    Backtick(String),
    Op(&'static str),
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
    /// Whitespace precedes this token (distinguishes `cmd (x)` from `f(x)`).
    pub space: bool,
}

const OPS: [&str; 33] = [
    "<<", ">>", "==", "!=", "<=", ">=", "&&", "||", "+=", "-=", "*=", "/=", "++", "--", "+", "-", "*", "/", "%", "<", ">", "=",
    "!", "(", ")", "{", "}", "[", "]", ";", ",", "?", ":",
];

pub fn lex(src: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut line = 1;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        let tok_start = i;
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // comments
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                if chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            i += 2;
            continue;
        }
        if c == '"' {
            let mut s = String::new();
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                    s.push(match chars[i] {
                        'n' => '\n',
                        't' => '\t',
                        other => other,
                    });
                } else {
                    s.push(chars[i]);
                }
                i += 1;
            }
            if i >= chars.len() {
                return Err(format!("line {line}: unterminated string"));
            }
            i += 1;
            out.push(tk(&chars, tok_start, Token { space: false, tok: Tok::Str(s), line }));
            continue;
        }
        if c == '`' {
            let mut s = String::new();
            i += 1;
            while i < chars.len() && chars[i] != '`' {
                s.push(chars[i]);
                i += 1;
            }
            i += 1;
            out.push(tk(&chars, tok_start, Token { space: false, tok: Tok::Backtick(s), line }));
            continue;
        }
        if c == '$' {
            let mut s = String::new();
            i += 1;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                s.push(chars[i]);
                i += 1;
            }
            out.push(tk(&chars, tok_start, Token { space: false, tok: Tok::Var(s), line }));
            continue;
        }
        let prev_ws = i == 0 || chars[i - 1].is_whitespace() || chars[i - 1] == '(' || chars[i - 1] == ',';
        if c == '-' && prev_ws && chars.get(i + 1).is_some_and(|n| n.is_ascii_alphabetic()) {
            // a flag like -name (only when it follows whitespace)
            let mut s = String::new();
            i += 1;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                s.push(chars[i]);
                i += 1;
            }
            out.push(tk(&chars, tok_start, Token { space: false, tok: Tok::Flag(s), line }));
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit())) {
            let start = i;
            let mut is_int = true;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                if chars[i] == '.' {
                    is_int = false;
                }
                i += 1;
            }
            if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                let save = i;
                i += 1;
                if i < chars.len() && (chars[i] == '-' || chars[i] == '+') {
                    i += 1;
                }
                if i < chars.len() && chars[i].is_ascii_digit() {
                    is_int = false;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                } else {
                    i = save;
                }
            }
            // a number immediately followed by letters is a word (e.g. 2d)
            if i < chars.len() && (chars[i].is_alphabetic() || chars[i] == '_') {
                while i < chars.len() && is_word_char(chars[i]) {
                    i += 1;
                }
                out.push(tk(&chars, tok_start, Token { space: false, tok: Tok::Word(chars[start..i].iter().collect()), line }));
                continue;
            }
            let s: String = chars[start..i].iter().collect();
            let v: f64 = s.parse().map_err(|_| format!("line {line}: bad number {s}"))?;
            out.push(tk(&chars, tok_start, Token { space: false, tok: Tok::Num(v, is_int), line }));
            continue;
        }
        if c.is_alphabetic()
            || c == '_'
            || (c == '|' && chars.get(i + 1).is_some_and(|n| n.is_alphabetic()))
            || (c == '*' && prev_ws && chars.get(i + 1).is_some_and(|n| n.is_alphanumeric()))
        {
            let start = i;
            while i < chars.len() && (is_word_char(chars[i]) || (chars[i] == '[' && word_index_follows(&chars, i))) {
                if chars[i] == '[' {
                    while i < chars.len() && chars[i] != ']' {
                        i += 1;
                    }
                }
                i += 1;
            }
            out.push(tk(&chars, tok_start, Token { space: false, tok: Tok::Word(chars[start..i].iter().collect()), line }));
            continue;
        }
        let rest: String = chars[i..chars.len().min(i + 2)].iter().collect();
        if let Some(op) = OPS.iter().find(|op| rest.starts_with(**op)) {
            out.push(tk(&chars, tok_start, Token { space: false, tok: Tok::Op(op), line }));
            i += op.len();
            continue;
        }
        return Err(format!("line {line}: unexpected character '{c}'"));
    }
    out.push(Token { tok: Tok::Eof, line, space: true });
    Ok(out)
}

fn tk(chars: &[char], start: usize, mut t: Token) -> Token {
    t.space = start == 0 || chars[start - 1].is_whitespace();
    t
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.' || c == '|' || c == ':' || c == '*'
}

/// `pCube1.f[0:3]` — brackets with digits/colon right after a dotted word.
fn word_index_follows(chars: &[char], i: usize) -> bool {
    // only treat as part of the word if previous text had a '.' (component)
    let mut j = i;
    while j > 0 && is_word_char(chars[j - 1]) {
        j -= 1;
    }
    let word: String = chars[j..i].iter().collect();
    if !word.contains('.') {
        return false;
    }
    let mut k = i + 1;
    while k < chars.len() && (chars[k].is_ascii_digit() || chars[k] == ':' || chars[k] == '*') {
        k += 1;
    }
    k < chars.len() && chars[k] == ']'
}

// ---------------------------------------------------------------- AST ---

#[derive(Clone, Debug)]
pub enum Expr {
    Num(f64, bool),
    Str(String),
    Var(String, Option<Box<Expr>>, Option<char>),
    Vector(Box<[Expr; 3]>),
    Array(Vec<Expr>),
    Unary(&'static str, Box<Expr>),
    Binary(&'static str, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    Command(CmdCall),
    Assign(String, Option<Box<Expr>>, &'static str, Box<Expr>),
    IncDec(String, f64, bool), // var, delta, prefix
}

#[derive(Clone, Debug)]
pub enum CmdArg {
    Flag(String),
    Val(Expr),
}

#[derive(Clone, Debug)]
pub struct CmdCall {
    pub name: String,
    pub args: Vec<CmdArg>,
    pub line: usize,
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Expr(Expr, usize),
    Decl { ty: String, name: String, array: bool, init: Option<Expr>, global: bool },
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    While(Expr, Box<Stmt>),
    DoWhile(Box<Stmt>, Expr),
    For(Option<Box<Stmt>>, Option<Expr>, Option<Expr>, Box<Stmt>),
    ForIn(String, Expr, Box<Stmt>),
    Block(Vec<Stmt>),
    Proc(Proc),
    Return(Option<Expr>),
    Break,
    Continue,
    Switch(Expr, Vec<(Option<Expr>, Vec<Stmt>)>),
    Empty,
}

#[derive(Clone, Debug)]
pub struct Proc {
    pub name: String,
    pub params: Vec<(String, String, bool)>,
    pub body: Vec<Stmt>,
}

pub struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

const TYPES: [&str; 5] = ["int", "float", "string", "vector", "matrix"];

impl Parser {
    pub fn new(toks: Vec<Token>) -> Self {
        Self { toks, pos: 0 }
    }

    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }
    fn peek_at(&self, n: usize) -> &Tok {
        &self.toks[(self.pos + n).min(self.toks.len() - 1)].tok
    }
    fn line(&self) -> usize {
        self.toks[self.pos].line
    }
    fn next(&mut self) -> Tok {
        let t = self.toks[self.pos].tok.clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }
    fn is_op(&self, op: &str) -> bool {
        matches!(self.peek(), Tok::Op(o) if *o == op)
    }
    fn eat_op(&mut self, op: &str) -> bool {
        if self.is_op(op) {
            self.next();
            true
        } else {
            false
        }
    }
    fn expect_op(&mut self, op: &str) -> Result<(), String> {
        if self.eat_op(op) {
            Ok(())
        } else {
            Err(format!("line {}: expected '{op}' but found {}", self.line(), describe(self.peek())))
        }
    }

    pub fn program(&mut self) -> Result<Vec<Stmt>, String> {
        let mut out = Vec::new();
        while *self.peek() != Tok::Eof {
            out.push(self.stmt()?);
        }
        Ok(out)
    }

    fn block_or_stmt(&mut self) -> Result<Stmt, String> {
        self.stmt()
    }

    fn stmt(&mut self) -> Result<Stmt, String> {
        let line = self.line();
        match self.peek().clone() {
            Tok::Op(";") => {
                self.next();
                Ok(Stmt::Empty)
            }
            Tok::Op("{") => {
                self.next();
                let mut v = Vec::new();
                while !self.is_op("}") {
                    if *self.peek() == Tok::Eof {
                        return Err(format!("line {line}: missing '}}'"));
                    }
                    v.push(self.stmt()?);
                }
                self.next();
                Ok(Stmt::Block(v))
            }
            Tok::Word(w) => match w.as_str() {
                "if" => {
                    self.next();
                    self.expect_op("(")?;
                    let c = self.expr()?;
                    self.expect_op(")")?;
                    let a = self.block_or_stmt()?;
                    let b = if matches!(self.peek(), Tok::Word(w) if w == "else") {
                        self.next();
                        Some(Box::new(self.block_or_stmt()?))
                    } else {
                        None
                    };
                    Ok(Stmt::If(c, Box::new(a), b))
                }
                "while" => {
                    self.next();
                    self.expect_op("(")?;
                    let c = self.expr()?;
                    self.expect_op(")")?;
                    Ok(Stmt::While(c, Box::new(self.block_or_stmt()?)))
                }
                "do" => {
                    self.next();
                    let body = self.block_or_stmt()?;
                    match self.next() {
                        Tok::Word(w) if w == "while" => {}
                        t => return Err(format!("line {line}: expected while, found {}", describe(&t))),
                    }
                    self.expect_op("(")?;
                    let c = self.expr()?;
                    self.expect_op(")")?;
                    self.eat_op(";");
                    Ok(Stmt::DoWhile(Box::new(body), c))
                }
                "for" => {
                    self.next();
                    self.expect_op("(")?;
                    // for ($x in $list)
                    if let (Tok::Var(v), Tok::Word(w)) = (self.peek().clone(), self.peek_at(1).clone()) {
                        if w == "in" {
                            self.next();
                            self.next();
                            let e = self.expr()?;
                            self.expect_op(")")?;
                            return Ok(Stmt::ForIn(v, e, Box::new(self.block_or_stmt()?)));
                        }
                    }
                    let init = if self.is_op(";") { None } else { Some(Box::new(self.simple_stmt()?)) };
                    self.expect_op(";")?;
                    let cond = if self.is_op(";") { None } else { Some(self.expr()?) };
                    self.expect_op(";")?;
                    let step = if self.is_op(")") { None } else { Some(self.expr()?) };
                    self.expect_op(")")?;
                    Ok(Stmt::For(init, cond, step, Box::new(self.block_or_stmt()?)))
                }
                "switch" => {
                    self.next();
                    self.expect_op("(")?;
                    let e = self.expr()?;
                    self.expect_op(")")?;
                    self.expect_op("{")?;
                    let mut cases = Vec::new();
                    while !self.eat_op("}") {
                        let label = match self.next() {
                            Tok::Word(w) if w == "case" => {
                                let v = self.expr()?;
                                Some(v)
                            }
                            Tok::Word(w) if w == "default" => None,
                            t => return Err(format!("line {line}: expected case, found {}", describe(&t))),
                        };
                        self.expect_op(":")?;
                        let mut body = Vec::new();
                        while !matches!(self.peek(), Tok::Word(w) if w == "case" || w == "default") && !self.is_op("}") {
                            body.push(self.stmt()?);
                        }
                        cases.push((label, body));
                    }
                    Ok(Stmt::Switch(e, cases))
                }
                "proc" | "global" if w == "proc" || matches!(self.peek_at(1), Tok::Word(x) if x == "proc") => {
                    if w == "global" {
                        self.next();
                    }
                    self.next();
                    // optional return type
                    if let Tok::Word(t) = self.peek().clone() {
                        if TYPES.contains(&t.as_str()) && !matches!(self.peek_at(1), Tok::Op("(")) {
                            self.next();
                            self.eat_op("[");
                            self.eat_op("]");
                        }
                    }
                    let name = match self.next() {
                        Tok::Word(n) => n,
                        t => return Err(format!("line {line}: expected proc name, found {}", describe(&t))),
                    };
                    self.expect_op("(")?;
                    let mut params = Vec::new();
                    while !self.eat_op(")") {
                        let ty = match self.next() {
                            Tok::Word(t) => t,
                            t => return Err(format!("line {line}: expected parameter type, found {}", describe(&t))),
                        };
                        let name = match self.next() {
                            Tok::Var(v) => v,
                            t => return Err(format!("line {line}: expected parameter, found {}", describe(&t))),
                        };
                        let arr = self.eat_op("[");
                        if arr {
                            self.expect_op("]")?;
                        }
                        params.push((ty, name, arr));
                        self.eat_op(",");
                    }
                    let body = match self.stmt()? {
                        Stmt::Block(b) => b,
                        s => vec![s],
                    };
                    Ok(Stmt::Proc(Proc { name, params, body }))
                }
                "return" => {
                    self.next();
                    let e = if self.is_op(";") { None } else { Some(self.expr()?) };
                    self.eat_op(";");
                    Ok(Stmt::Return(e))
                }
                "break" => {
                    self.next();
                    self.eat_op(";");
                    Ok(Stmt::Break)
                }
                "continue" => {
                    self.next();
                    self.eat_op(";");
                    Ok(Stmt::Continue)
                }
                _ => {
                    let s = self.simple_stmt()?;
                    if !self.eat_op(";") && *self.peek() != Tok::Eof && !self.is_op("}") {
                        return Err(format!("line {}: expected ';' but found {}", self.line(), describe(self.peek())));
                    }
                    Ok(s)
                }
            },
            _ => {
                let s = self.simple_stmt()?;
                if !self.eat_op(";") && *self.peek() != Tok::Eof && !self.is_op("}") {
                    return Err(format!("line {}: expected ';' but found {}", self.line(), describe(self.peek())));
                }
                Ok(s)
            }
        }
    }

    /// Declaration, command or expression (without the trailing ';').
    fn simple_stmt(&mut self) -> Result<Stmt, String> {
        let line = self.line();
        if let Tok::Word(w) = self.peek().clone() {
            let global = w == "global";
            let tyw = if global {
                match self.peek_at(1) {
                    Tok::Word(t) => t.clone(),
                    _ => String::new(),
                }
            } else {
                w.clone()
            };
            if TYPES.contains(&tyw.as_str()) && matches!(self.peek_at(if global { 2 } else { 1 }), Tok::Var(_)) {
                if global {
                    self.next();
                }
                self.next();
                let mut decls = Vec::new();
                loop {
                    let name = match self.next() {
                        Tok::Var(v) => v,
                        t => return Err(format!("line {line}: expected variable, found {}", describe(&t))),
                    };
                    let mut array = false;
                    if self.eat_op("[") {
                        array = true;
                        while !self.eat_op("]") {
                            self.next();
                        }
                    }
                    let init = if self.eat_op("=") { Some(self.expr()?) } else { None };
                    decls.push(Stmt::Decl { ty: tyw.clone(), name, array, init, global });
                    if !self.eat_op(",") {
                        break;
                    }
                }
                return Ok(if decls.len() == 1 { decls.pop().unwrap() } else { Stmt::Block(decls) });
            }
            // command syntax: word not followed by '(' or '='
            let paren_call = matches!(self.peek_at(1), Tok::Op("(")) && !self.toks[(self.pos + 1).min(self.toks.len() - 1)].space;
            if !paren_call && !matches!(self.peek_at(1), Tok::Op("=")) {
                self.next();
                let call = self.command_args(w, line)?;
                return Ok(Stmt::Expr(Expr::Command(call), line));
            }
        }
        Ok(Stmt::Expr(self.expr()?, line))
    }

    fn command_args(&mut self, name: String, line: usize) -> Result<CmdCall, String> {
        let mut args = Vec::new();
        loop {
            match self.peek().clone() {
                Tok::Op(";") | Tok::Op("}") | Tok::Eof => break,
                Tok::Op(")") => break,
                Tok::Flag(f) => {
                    self.next();
                    args.push(CmdArg::Flag(f));
                }
                Tok::Op("(") => {
                    self.next();
                    let e = self.expr()?;
                    self.expect_op(")")?;
                    args.push(CmdArg::Val(e));
                }
                Tok::Op("-") => {
                    // negative number literal
                    self.next();
                    match self.next() {
                        Tok::Num(n, i) => args.push(CmdArg::Val(Expr::Num(-n, i))),
                        Tok::Var(v) => args.push(CmdArg::Val(Expr::Unary("-", Box::new(Expr::Var(v, None, None))))),
                        t => return Err(format!("line {line}: unexpected {} after '-'", describe(&t))),
                    }
                }
                Tok::Op("<<") => args.push(CmdArg::Val(self.primary()?)),
                Tok::Op("{") => args.push(CmdArg::Val(self.primary()?)),
                Tok::Num(n, i) => {
                    self.next();
                    args.push(CmdArg::Val(Expr::Num(n, i)));
                }
                Tok::Str(s) => {
                    self.next();
                    args.push(CmdArg::Val(Expr::Str(s)));
                }
                Tok::Word(w) => {
                    self.next();
                    args.push(CmdArg::Val(Expr::Str(w)));
                }
                Tok::Var(_) => args.push(CmdArg::Val(self.postfix()?)),
                Tok::Backtick(b) => {
                    self.next();
                    args.push(CmdArg::Val(parse_backtick(&b, line)?));
                }
                t => return Err(format!("line {line}: unexpected {} in {name} arguments", describe(&t))),
            }
        }
        Ok(CmdCall { name, args, line })
    }

    pub fn expr(&mut self) -> Result<Expr, String> {
        // assignment
        if let Tok::Var(v) = self.peek().clone() {
            let save = self.pos;
            self.next();
            let idx = if self.eat_op("[") {
                let e = self.expr()?;
                self.expect_op("]")?;
                Some(Box::new(e))
            } else {
                None
            };
            for op in ["=", "+=", "-=", "*=", "/="] {
                if self.is_op(op) {
                    self.next();
                    let rhs = self.expr()?;
                    let sop: &'static str = OPS.iter().find(|o| **o == op).unwrap();
                    return Ok(Expr::Assign(v, idx, sop, Box::new(rhs)));
                }
            }
            self.pos = save;
        }
        self.ternary()
    }

    fn ternary(&mut self) -> Result<Expr, String> {
        let c = self.binary(0)?;
        if self.eat_op("?") {
            let a = self.expr()?;
            self.expect_op(":")?;
            let b = self.expr()?;
            return Ok(Expr::Ternary(Box::new(c), Box::new(a), Box::new(b)));
        }
        Ok(c)
    }

    fn binary(&mut self, min_prec: u8) -> Result<Expr, String> {
        let mut lhs = self.unary()?;
        loop {
            let (op, prec) = match self.peek() {
                Tok::Op(o) => match *o {
                    "||" => ("||", 1),
                    "&&" => ("&&", 2),
                    "==" | "!=" => (*o, 3),
                    "<" | ">" | "<=" | ">=" => (*o, 4),
                    "+" | "-" => (*o, 5),
                    "*" | "/" | "%" => (*o, 6),
                    _ => break,
                },
                // a flag token inside an expression is really "minus word"
                _ => break,
            };
            if prec < min_prec || prec == 0 {
                break;
            }
            self.next();
            let rhs = self.binary(prec + 1)?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.eat_op("-") {
            return Ok(Expr::Unary("-", Box::new(self.unary()?)));
        }
        if self.eat_op("!") {
            return Ok(Expr::Unary("!", Box::new(self.unary()?)));
        }
        if self.eat_op("+") {
            return self.unary();
        }
        if self.is_op("++") || self.is_op("--") {
            let d = if self.is_op("++") { 1.0 } else { -1.0 };
            self.next();
            if let Tok::Var(v) = self.next() {
                return Ok(Expr::IncDec(v, d, true));
            }
            return Err(format!("line {}: ++/-- needs a variable", self.line()));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr, String> {
        let e = self.primary()?;
        if let Expr::Var(name, None, None) = &e {
            if self.is_op("++") || self.is_op("--") {
                let d = if self.is_op("++") { 1.0 } else { -1.0 };
                self.next();
                return Ok(Expr::IncDec(name.clone(), d, false));
            }
        }
        Ok(e)
    }

    fn primary(&mut self) -> Result<Expr, String> {
        let line = self.line();
        match self.next() {
            Tok::Num(n, i) => Ok(Expr::Num(n, i)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::Backtick(b) => parse_backtick(&b, line),
            Tok::Var(v) => {
                let idx = if self.eat_op("[") {
                    let e = self.expr()?;
                    self.expect_op("]")?;
                    Some(Box::new(e))
                } else {
                    None
                };
                Ok(Expr::Var(v, idx, None))
            }
            Tok::Op("(") => {
                let e = self.expr()?;
                self.expect_op(")")?;
                Ok(e)
            }
            Tok::Op("{") => {
                let mut items = Vec::new();
                while !self.eat_op("}") {
                    items.push(self.expr()?);
                    self.eat_op(",");
                }
                Ok(Expr::Array(items))
            }
            Tok::Op("<<") => {
                let a = self.binary(0)?;
                self.expect_op(",")?;
                let b = self.binary(0)?;
                self.expect_op(",")?;
                let c = self.binary(0)?;
                self.expect_op(">>")?;
                Ok(Expr::Vector(Box::new([a, b, c])))
            }
            Tok::Word(w) => {
                // variable component access looks like a word: handled in Var
                if self.eat_op("(") {
                    let mut args = Vec::new();
                    while !self.eat_op(")") {
                        args.push(self.expr()?);
                        if !self.eat_op(",") && !self.is_op(")") {
                            return Err(format!("line {line}: expected ',' or ')' in call to {w}"));
                        }
                    }
                    Ok(Expr::Call(w, args))
                } else if w == "true" || w == "on" || w == "yes" {
                    Ok(Expr::Num(1.0, true))
                } else if w == "false" || w == "off" || w == "no" {
                    Ok(Expr::Num(0.0, true))
                } else {
                    Ok(Expr::Str(w))
                }
            }
            Tok::Flag(f) => Ok(Expr::Unary("-", Box::new(Expr::Str(f)))),
            t => Err(format!("line {line}: unexpected {}", describe(&t))),
        }
    }
}

fn parse_backtick(src: &str, line: usize) -> Result<Expr, String> {
    let toks = lex(src).map_err(|e| format!("in backticks on line {line}: {e}"))?;
    let mut p = Parser::new(toks);
    let name = match p.next() {
        Tok::Word(w) => w,
        t => return Err(format!("line {line}: backticks must contain a command, found {}", describe(&t))),
    };
    let call = p.command_args(name, line)?;
    Ok(Expr::Command(call))
}

pub fn describe(t: &Tok) -> String {
    match t {
        Tok::Word(w) => format!("'{w}'"),
        Tok::Var(v) => format!("'${v}'"),
        Tok::Num(n, _) => format!("number {n}"),
        Tok::Str(s) => format!("string \"{s}\""),
        Tok::Flag(f) => format!("flag -{f}"),
        Tok::Backtick(_) => "backtick command".into(),
        Tok::Op(o) => format!("'{o}'"),
        Tok::Eof => "end of input".into(),
    }
}

pub fn parse(src: &str) -> Result<Vec<Stmt>, String> {
    let toks = lex(src)?;
    Parser::new(toks).program()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_flags_and_numbers() {
        let t = lex("move -r -1.5 2 3 pCube1.tx;").unwrap();
        assert!(matches!(&t[1].tok, Tok::Flag(f) if f == "r"));
        assert!(matches!(&t[2].tok, Tok::Op("-")));
        assert!(matches!(&t[3].tok, Tok::Num(n, false) if (*n - 1.5).abs() < 1e-9));
        assert!(matches!(&t[6].tok, Tok::Word(w) if w == "pCube1.tx"));
    }

    #[test]
    fn components_lex_as_words() {
        let t = lex("select pCube1.f[0:3] pCube1.vtx[2];").unwrap();
        assert!(matches!(&t[1].tok, Tok::Word(w) if w == "pCube1.f[0:3]"));
        assert!(matches!(&t[2].tok, Tok::Word(w) if w == "pCube1.vtx[2]"));
    }

    #[test]
    fn parses_program() {
        let p = parse(
            r#"
            string $objs[] = `ls -sl`;
            for ($i = 0; $i < 3; $i++) { polyCube -w ($i + 1); }
            for ($o in $objs) print($o + "\n");
            proc float twice(float $x) { return $x * 2; }
            if (twice(2) > 3) print "big"; else print "small";
            "#,
        )
        .unwrap();
        assert_eq!(p.len(), 5);
    }
}
