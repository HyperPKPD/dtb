//! The expression language of Prep Studio: a small, safe language for derived columns and row filters. No eval, no access to anything
//! but the columns of the table. Values are numbers, text, true/false or missing. This is a port of the JavaScript engine of the
//! app; both give the same values for the same table.

use super::table::{Cell, Frame};
use crate::core::js;
use std::collections::HashMap;

const GROUP_FNS: &[&str] = &[
    "lag", "lead", "first", "last", "cumsum", "cummax", "cummin", "rownum", "count", "n", "gsum", "gmean", "gmedian", "gmin", "gmax", "gcount", "gany", "gall",
    "locf", "nocb",
];
const ROW_FNS: &[&str] = &[
    "if",
    "ifelse",
    "abs",
    "sqrt",
    "exp",
    "log",
    "ln",
    "log10",
    "log2",
    "round",
    "floor",
    "ceil",
    "trunc",
    "min",
    "max",
    "isna",
    "coalesce",
    "num",
    "str",
    "upper",
    "lower",
    "trim",
    "substr",
    "len",
    "contains",
    "startswith",
    "endswith",
    "replace",
    "concat",
    "pow",
    "sign",
    "between",
    "in",
    "pi",
    "sin",
    "cos",
    "tan",
];

pub fn is_group_fn(n: &str) -> bool {
    GROUP_FNS.contains(&n)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Null,
    Bool(bool),
    Num(f64),
    Text(String),
}

#[derive(Debug, Clone)]
pub struct ExprError {
    pub message: String,
    pub pos: usize,
}
impl std::fmt::Display for ExprError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
fn err<T>(m: impl Into<String>, pos: usize) -> Result<T, ExprError> {
    Err(ExprError { message: m.into(), pos })
}

// ------------------------------------------------------------------------------------------------ tokens

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Col(String),
    Id(String),
    Op(&'static str),
    End,
}

fn tokenize(src: &str) -> Result<Vec<(Tok, usize)>, ExprError> {
    let ch: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < ch.len() {
        let c = ch[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && ch.get(i + 1).is_some_and(|x| x.is_ascii_digit())) {
            let s = i;
            while i < ch.len() && ch[i].is_ascii_digit() {
                i += 1;
            }
            if i < ch.len() && ch[i] == '.' {
                i += 1;
                while i < ch.len() && ch[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if i < ch.len() && (ch[i] == 'e' || ch[i] == 'E') {
                let mut k = i + 1;
                if k < ch.len() && (ch[k] == '+' || ch[k] == '-') {
                    k += 1;
                }
                if k < ch.len() && ch[k].is_ascii_digit() {
                    while k < ch.len() && ch[k].is_ascii_digit() {
                        k += 1;
                    }
                    i = k;
                }
            }
            let text: String = ch[s..i].iter().collect();
            out.push((Tok::Num(js::number(&text)), s));
            continue;
        }
        if c == '"' || c == '\'' {
            let mut j = i + 1;
            let mut s = String::new();
            while j < ch.len() && ch[j] != c {
                if ch[j] == '\\' && j + 1 < ch.len() {
                    j += 1;
                    s.push(match ch[j] {
                        'n' => '\n',
                        't' => '\t',
                        o => o,
                    });
                } else {
                    s.push(ch[j]);
                }
                j += 1;
            }
            if j >= ch.len() {
                return err("A quoted text is not closed.", i);
            }
            out.push((Tok::Str(s), i));
            i = j + 1;
            continue;
        }
        if c == '[' || c == '`' {
            let close = if c == '[' { ']' } else { '`' };
            let Some(off) = ch[i + 1..].iter().position(|x| *x == close) else { return err(format!("A column name that starts with {c} is not closed."), i) };
            out.push((Tok::Col(ch[i + 1..i + 1 + off].iter().collect()), i));
            i += off + 2;
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let s = i;
            while i < ch.len() && (ch[i].is_ascii_alphanumeric() || ch[i] == '_' || ch[i] == '.') {
                i += 1;
            }
            out.push((Tok::Id(ch[s..i].iter().collect()), s));
            continue;
        }
        if i + 1 < ch.len() {
            let two: String = ch[i..i + 2].iter().collect();
            let op = match two.as_str() {
                "==" => Some("=="),
                "!=" | "<>" => Some("!="),
                "<=" => Some("<="),
                ">=" => Some(">="),
                "&&" => Some("&&"),
                "||" => Some("||"),
                _ => None,
            };
            if let Some(o) = op {
                out.push((Tok::Op(o), i));
                i += 2;
                continue;
            }
        }
        let one = match c {
            '+' => "+",
            '-' => "-",
            '*' => "*",
            '/' => "/",
            '%' => "%",
            '^' => "^",
            '<' => "<",
            '>' => ">",
            '(' => "(",
            ')' => ")",
            '!' => "!",
            ',' => ",",
            '=' => "==",
            _ => return err(format!("Unexpected character \"{c}\"."), i),
        };
        out.push((Tok::Op(one), i));
        i += 1;
    }
    out.push((Tok::End, ch.len()));
    Ok(out)
}

// ------------------------------------------------------------------------------------------------ parser

#[derive(Debug, Clone)]
pub enum Node {
    Lit(Val),
    Col { name: String, pos: usize },
    Un { op: &'static str, a: Box<Node> },
    Bin { op: &'static str, a: Box<Node>, b: Box<Node> },
    Call { name: String, args: Vec<Node>, pos: usize, src: String, id: usize },
}

fn prec(op: &str) -> Option<u8> {
    Some(match op {
        "||" => 1,
        "&&" => 2,
        "==" | "!=" => 3,
        "<" | "<=" | ">" | ">=" => 4,
        "+" | "-" => 5,
        "*" | "/" | "%" => 6,
        "^" => 8,
        _ => return None,
    })
}

struct Parser {
    toks: Vec<(Tok, usize)>,
    pos: usize,
    calls: usize,
}

impl Parser {
    fn peek(&self) -> &(Tok, usize) {
        &self.toks[self.pos]
    }
    fn next(&mut self) -> (Tok, usize) {
        let t = self.toks[self.pos].clone();
        self.pos += 1;
        t
    }
    fn is_op(&self, v: &str) -> bool {
        matches!(&self.peek().0, Tok::Op(o) if *o == v)
    }
    fn kw(&self) -> String {
        if let Tok::Id(s) = &self.peek().0 {
            s.to_lowercase()
        } else {
            String::new()
        }
    }
    fn expr(&mut self, min: u8) -> Result<Node, ExprError> {
        let mut left = self.unary()?;
        loop {
            let op: Option<&'static str> = match &self.peek().0 {
                Tok::Op(o) if prec(o).is_some() => Some(o),
                Tok::Id(s) if s.eq_ignore_ascii_case("and") => Some("&&"),
                Tok::Id(s) if s.eq_ignore_ascii_case("or") => Some("||"),
                _ => None,
            };
            let Some(op) = op else { break };
            let p = prec(op).unwrap();
            if p < min {
                break;
            }
            self.next();
            let right = self.expr(if op == "^" { p } else { p + 1 })?;
            left = Node::Bin { op, a: Box::new(left), b: Box::new(right) };
        }
        Ok(left)
    }
    fn unary(&mut self) -> Result<Node, ExprError> {
        let (t, _) = self.peek().clone();
        if let Tok::Op(o) = &t {
            if *o == "-" || *o == "+" {
                self.next();
                let a = self.expr(7)?;
                return Ok(if *o == "-" { Node::Un { op: "-", a: Box::new(a) } } else { a });
            }
            if *o == "!" {
                self.next();
                return Ok(Node::Un { op: "!", a: Box::new(self.expr(7)?) });
            }
        }
        if self.kw() == "not" {
            self.next();
            return Ok(Node::Un { op: "!", a: Box::new(self.expr(7)?) });
        }
        self.primary()
    }
    fn primary(&mut self) -> Result<Node, ExprError> {
        let (t, p) = self.next();
        match t {
            Tok::Num(v) => Ok(Node::Lit(Val::Num(v))),
            Tok::Str(s) => Ok(Node::Lit(Val::Text(s))),
            Tok::Col(c) => Ok(Node::Col { name: c, pos: p }),
            Tok::Op("(") => {
                let e = self.expr(1)?;
                if !self.is_op(")") {
                    return err("A closing ) is missing.", self.peek().1);
                }
                self.next();
                Ok(e)
            }
            Tok::Id(name) => {
                let low = name.to_lowercase();
                if self.is_op("(") {
                    self.next();
                    let mut args = Vec::new();
                    if !self.is_op(")") {
                        loop {
                            args.push(self.expr(1)?);
                            if self.is_op(",") {
                                self.next();
                                continue;
                            }
                            break;
                        }
                    }
                    if !self.is_op(")") {
                        return err(format!("A closing ) is missing after {name}(…"), self.peek().1);
                    }
                    self.next();
                    let id = self.calls;
                    self.calls += 1;
                    return Ok(Node::Call { name: low, args, pos: p, src: name, id });
                }
                match low.as_str() {
                    "true" => Ok(Node::Lit(Val::Bool(true))),
                    "false" => Ok(Node::Lit(Val::Bool(false))),
                    "na" | "null" | "nan" | "missing" => Ok(Node::Lit(Val::Null)),
                    _ => Ok(Node::Col { name, pos: p }),
                }
            }
            Tok::End => err("The expression ends too early.", p),
            Tok::Op(o) => err(format!("Unexpected \"{o}\"."), p),
        }
    }
}

/// Parse an expression; also returns the number of function calls (each gets an id).
pub fn parse(src: &str) -> Result<(Node, usize), ExprError> {
    let toks = tokenize(src)?;
    if toks.len() == 1 {
        return err("The expression is empty.", 0);
    }
    let mut p = Parser { toks, pos: 0, calls: 0 };
    let e = p.expr(1)?;
    if p.peek().0 != Tok::End {
        let v = match &p.peek().0 {
            Tok::Op(o) => o.to_string(),
            Tok::Id(s) | Tok::Str(s) | Tok::Col(s) => s.clone(),
            Tok::Num(n) => js::num_str(*n),
            Tok::End => String::new(),
        };
        return err(format!("Unexpected \"{v}\"."), p.peek().1);
    }
    Ok((e, p.calls))
}

// ------------------------------------------------------------------------------------------------ values

pub fn to_num(v: &Val) -> Option<f64> {
    match v {
        Val::Num(x) => x.is_finite().then_some(*x),
        Val::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Val::Text(s) => {
            if super::table::is_plain_number(s.trim()) {
                Some(js::number(s))
            } else {
                None
            }
        }
        Val::Null => None,
    }
}
pub fn truthy(v: &Val) -> bool {
    match v {
        Val::Null => false,
        Val::Bool(b) => *b,
        Val::Num(x) => *x != 0.0 && !x.is_nan(),
        Val::Text(s) => !s.is_empty(),
    }
}
pub fn is_missing(v: &Val) -> bool {
    match v {
        Val::Null => true,
        Val::Num(x) => !x.is_finite(),
        _ => false,
    }
}
pub fn text(v: &Val) -> String {
    match v {
        Val::Null => String::new(),
        Val::Num(x) => js::num_str(*x),
        Val::Bool(b) => b.to_string(),
        Val::Text(s) => s.clone(),
    }
}
fn from_cell(c: &Cell) -> Val {
    match c {
        Cell::Null => Val::Null,
        Cell::Num(x) => Val::Num(*x),
        Cell::Text(s) => Val::Text(s.clone()),
    }
}
/// A stored value from an evaluated one: numbers stay numbers, true/false become 1/0, missing stays missing.
pub fn to_cell(v: &Val) -> Cell {
    match v {
        Val::Null => Cell::Null,
        Val::Bool(b) => Cell::Num(if *b { 1.0 } else { 0.0 }),
        Val::Num(x) => {
            if x.is_finite() {
                Cell::Num(*x)
            } else {
                Cell::Null
            }
        }
        Val::Text(s) => Cell::Text(s.clone()),
    }
}

fn compare(a: &Val, b: &Val, op: &str) -> bool {
    if is_missing(a) || is_missing(b) {
        return op == "!=" && !(is_missing(a) && is_missing(b));
    }
    let c: f64 = match (to_num(a), to_num(b)) {
        (Some(x), Some(y)) => x - y,
        _ => {
            let (x, y) = (text(a), text(b));
            if x < y {
                -1.0
            } else if x > y {
                1.0
            } else {
                0.0
            }
        }
    };
    match op {
        "==" => c == 0.0,
        "!=" => c != 0.0,
        "<" => c < 0.0,
        "<=" => c <= 0.0,
        ">" => c > 0.0,
        _ => c >= 0.0,
    }
}

// ------------------------------------------------------------------------------------------------ evaluation

fn median_sorted(s: &[f64]) -> Option<f64> {
    if s.is_empty() {
        None
    } else if s.len() % 2 == 1 {
        Some(s[(s.len() - 1) / 2])
    } else {
        Some((s[s.len() / 2 - 1] + s[s.len() / 2]) / 2.0)
    }
}

fn group_result(name: &str, vec: &[Val], keys: Option<&[String]>, extra: Option<f64>) -> Vec<Val> {
    let n = vec.len();
    let mut out = vec![Val::Null; n];
    let mut order: Vec<&str> = Vec::new();
    let mut groups: HashMap<&str, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let k = keys.map_or("", |k| k[i].as_str());
        groups.entry(k).or_insert_with(|| {
            order.push(k);
            Vec::new()
        });
        groups.get_mut(k).unwrap().push(i);
    }
    for k in order {
        let idx = &groups[k];
        let vals: Vec<&Val> = idx.iter().map(|i| &vec[*i]).collect();
        let nums: Vec<Option<f64>> = vals.iter().map(|v| to_num(v)).collect();
        let ok: Vec<f64> = nums.iter().flatten().copied().collect();
        let shift = |sign: i64| -> Option<i64> {
            let k = extra.unwrap_or(1.0);
            if k.fract() == 0.0 && k.abs() < 1e9 {
                Some(k as i64 * sign)
            } else {
                None
            }
        };
        match name {
            "lag" => {
                let s = shift(1);
                for (j, i) in idx.iter().enumerate() {
                    out[*i] = match s {
                        Some(k) if j as i64 - k >= 0 && ((j as i64 - k) as usize) < idx.len() => vals[(j as i64 - k) as usize].clone(),
                        _ => Val::Null,
                    };
                }
            }
            "lead" => {
                let s = shift(1);
                for (j, i) in idx.iter().enumerate() {
                    out[*i] = match s {
                        Some(k) if j as i64 + k >= 0 && ((j as i64 + k) as usize) < idx.len() => vals[(j as i64 + k) as usize].clone(),
                        _ => Val::Null,
                    };
                }
            }
            "first" => {
                let v = vals.iter().find(|x| !is_missing(x)).map_or(Val::Null, |x| (*x).clone());
                idx.iter().for_each(|i| out[*i] = v.clone());
            }
            "last" => {
                let v = vals.iter().rev().find(|x| !is_missing(x)).map_or(Val::Null, |x| (*x).clone());
                idx.iter().for_each(|i| out[*i] = v.clone());
            }
            "cumsum" => {
                let mut s = 0.0;
                for (j, i) in idx.iter().enumerate() {
                    if let Some(x) = nums[j] {
                        s += x;
                    }
                    out[*i] = Val::Num(s);
                }
            }
            "cummax" | "cummin" => {
                let mut s: Option<f64> = None;
                for (j, i) in idx.iter().enumerate() {
                    if let Some(x) = nums[j] {
                        if s.is_none_or(|c| if name == "cummax" { x > c } else { x < c }) {
                            s = Some(x);
                        }
                    }
                    out[*i] = s.map_or(Val::Null, Val::Num);
                }
            }
            "rownum" => idx.iter().enumerate().for_each(|(j, i)| out[*i] = Val::Num(j as f64 + 1.0)),
            "count" | "n" => idx.iter().for_each(|i| out[*i] = Val::Num(idx.len() as f64)),
            "gsum" => {
                let v = ok.iter().fold(0.0, |a, b| a + b);
                idx.iter().for_each(|i| out[*i] = Val::Num(v));
            }
            "gmean" => {
                let v = if ok.is_empty() { Val::Null } else { Val::Num(ok.iter().fold(0.0, |a, b| a + b) / ok.len() as f64) };
                idx.iter().for_each(|i| out[*i] = v.clone());
            }
            "gmedian" => {
                let mut s = ok.clone();
                s.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let v = median_sorted(&s).map_or(Val::Null, Val::Num);
                idx.iter().for_each(|i| out[*i] = v.clone());
            }
            "gmin" => {
                let v = ok.iter().copied().reduce(f64::min).map_or(Val::Null, Val::Num);
                idx.iter().for_each(|i| out[*i] = v.clone());
            }
            "gmax" => {
                let v = ok.iter().copied().reduce(f64::max).map_or(Val::Null, Val::Num);
                idx.iter().for_each(|i| out[*i] = v.clone());
            }
            "gcount" => {
                let v = vals.iter().filter(|x| truthy(x)).count() as f64;
                idx.iter().for_each(|i| out[*i] = Val::Num(v));
            }
            "gany" => {
                let v = vals.iter().any(|x| truthy(x));
                idx.iter().for_each(|i| out[*i] = Val::Bool(v));
            }
            "gall" => {
                let v = vals.iter().all(|x| truthy(x));
                idx.iter().for_each(|i| out[*i] = Val::Bool(v));
            }
            "locf" => {
                let mut cur = Val::Null;
                for (j, i) in idx.iter().enumerate() {
                    if !is_missing(vals[j]) {
                        cur = vals[j].clone();
                    }
                    out[*i] = cur.clone();
                }
            }
            "nocb" => {
                let mut cur = Val::Null;
                for j in (0..idx.len()).rev() {
                    if !is_missing(vals[j]) {
                        cur = vals[j].clone();
                    }
                    out[idx[j]] = cur.clone();
                }
            }
            _ => {}
        }
    }
    out
}

fn arg(a: &[Val], k: usize) -> &Val {
    a.get(k).unwrap_or(&Val::Null)
}

fn row_fn(name: &str, a: &[Val]) -> Val {
    let n = |k: usize| to_num(arg(a, k));
    let num = |v: Option<f64>| v.map_or(Val::Null, Val::Num);
    let unary = |f: &dyn Fn(f64) -> f64| num(n(0).map(f));
    match name {
        "abs" => unary(&f64::abs),
        "sqrt" => num(n(0).filter(|x| *x >= 0.0).map(f64::sqrt)),
        "exp" => unary(&f64::exp),
        "log" | "ln" => num(n(0).filter(|x| *x > 0.0).map(f64::ln)),
        "log10" => num(n(0).filter(|x| *x > 0.0).map(f64::log10)),
        "log2" => num(n(0).filter(|x| *x > 0.0).map(f64::log2)),
        "round" => match n(0) {
            None => Val::Null,
            Some(x) => {
                let d = n(1).unwrap_or(0.0);
                let f = 10f64.powf(d);
                Val::Num(js::round((x + f64::EPSILON * js::sign(x)) * f) / f)
            }
        },
        "floor" => unary(&f64::floor),
        "ceil" => unary(&f64::ceil),
        "trunc" => unary(&f64::trunc),
        "sign" => unary(&js::sign),
        "sin" => unary(&f64::sin),
        "cos" => unary(&f64::cos),
        "tan" => unary(&f64::tan),
        "pi" => Val::Num(std::f64::consts::PI),
        "pow" => match (n(0), n(1)) {
            (Some(x), Some(y)) => Val::Num(x.powf(y)),
            _ => Val::Null,
        },
        "min" => num(a.iter().filter_map(to_num).reduce(f64::min)),
        "max" => num(a.iter().filter_map(to_num).reduce(f64::max)),
        "isna" => Val::Bool(is_missing(arg(a, 0))),
        "coalesce" => a.iter().find(|v| !is_missing(v) && **v != Val::Text(String::new())).cloned().unwrap_or(Val::Null),
        "num" => match arg(a, 0) {
            Val::Text(s) => num(to_num(&Val::Text(s.trim().to_string()))),
            v => num(to_num(v)),
        },
        "str" => {
            if is_missing(arg(a, 0)) {
                Val::Null
            } else {
                Val::Text(text(arg(a, 0)))
            }
        }
        "upper" => {
            if is_missing(arg(a, 0)) {
                Val::Null
            } else {
                Val::Text(text(arg(a, 0)).to_uppercase())
            }
        }
        "lower" => {
            if is_missing(arg(a, 0)) {
                Val::Null
            } else {
                Val::Text(text(arg(a, 0)).to_lowercase())
            }
        }
        "trim" => {
            if is_missing(arg(a, 0)) {
                Val::Null
            } else {
                Val::Text(text(arg(a, 0)).trim().to_string())
            }
        }
        "len" => {
            if is_missing(arg(a, 0)) {
                Val::Null
            } else {
                Val::Num(text(arg(a, 0)).encode_utf16().count() as f64)
            }
        }
        "substr" => {
            if is_missing(arg(a, 0)) {
                return Val::Null;
            }
            let s: Vec<char> = text(arg(a, 0)).chars().collect();
            let st = (n(1).unwrap_or(1.0) - 1.0).max(0.0).trunc() as usize;
            let from = st.min(s.len());
            match n(2) {
                None => Val::Text(s[from..].iter().collect()),
                Some(l) => {
                    let l = l.trunc().max(0.0) as usize;
                    Val::Text(s[from..(from + l).min(s.len())].iter().collect())
                }
            }
        }
        "contains" => Val::Bool(text(arg(a, 0)).to_lowercase().contains(&text(arg(a, 1)).to_lowercase())),
        "startswith" => Val::Bool(text(arg(a, 0)).to_lowercase().starts_with(&text(arg(a, 1)).to_lowercase())),
        "endswith" => Val::Bool(text(arg(a, 0)).to_lowercase().ends_with(&text(arg(a, 1)).to_lowercase())),
        "replace" => {
            if is_missing(arg(a, 0)) {
                return Val::Null;
            }
            let (s, from, to) = (text(arg(a, 0)), text(arg(a, 1)), text(arg(a, 2)));
            if from.is_empty() {
                Val::Text(s.chars().map(String::from).collect::<Vec<_>>().join(&to))
            } else {
                Val::Text(s.replace(&from, &to))
            }
        }
        "concat" => Val::Text(a.iter().map(text).collect()),
        "between" => match (n(0), n(1), n(2)) {
            (Some(x), Some(lo), Some(hi)) => Val::Bool(x >= lo && x <= hi),
            _ => Val::Bool(false),
        },
        "in" => Val::Bool((1..a.len()).any(|k| compare(&a[0], &a[k], "=="))),
        _ => Val::Null,
    }
}

pub struct Evaluated {
    pub values: Vec<Val>,
    pub columns: Vec<String>,
}

fn arity(name: &str) -> Option<(usize, usize)> {
    Some(match name {
        "lag" | "lead" => (1, 2),
        "first" | "last" | "cumsum" | "cummax" | "cummin" | "gsum" | "gmean" | "gmedian" | "gmin" | "gmax" | "gcount" | "gany" | "gall" | "locf" | "nocb"
        | "isna" => (1, 1),
        "rownum" | "count" | "n" | "pi" => (0, 0),
        "if" | "ifelse" | "replace" | "between" => (3, 3),
        "round" => (1, 2),
        "substr" => (2, 3),
        _ => return None,
    })
}

struct Ctx<'a> {
    t: &'a Frame,
    col_idx: HashMap<&'a str, usize>,
    lower_idx: HashMap<String, usize>,
}

impl Ctx<'_> {
    fn lookup(&self, name: &str, pos: usize) -> Result<usize, ExprError> {
        if let Some(i) = self.col_idx.get(name) {
            return Ok(*i);
        }
        let l = name.to_lowercase();
        if let Some(i) = self.lower_idx.get(&l) {
            return Ok(*i);
        }
        let near: Vec<&str> =
            self.t.columns.iter().filter(|c| c.to_lowercase().contains(&l) || l.contains(&c.to_lowercase())).take(3).map(String::as_str).collect();
        err(format!("There is no column \"{name}\"{}.", if near.is_empty() { String::new() } else { format!(" (did you mean {}?)", near.join(", ")) }), pos)
    }
    fn check(&self, node: &Node, used: &mut Vec<String>) -> Result<(), ExprError> {
        match node {
            Node::Col { name, pos } => {
                let c = self.t.columns[self.lookup(name, *pos)?].clone();
                if !used.contains(&c) {
                    used.push(c);
                }
            }
            Node::Call { name, args, pos, src, .. } => {
                if !is_group_fn(name) && !ROW_FNS.contains(&name.as_str()) {
                    return err(
                        format!(
                            "Unknown function {src}(). Row functions: {}. Group functions: {}.",
                            ROW_FNS.iter().filter(|f| **f != "pi").copied().collect::<Vec<_>>().join(", "),
                            GROUP_FNS.join(", ")
                        ),
                        *pos,
                    );
                }
                if let Some((lo, hi)) = arity(name) {
                    if args.len() < lo || args.len() > hi {
                        return err(
                            format!("{src}() takes {} argument(s), not {}.", if lo == hi { lo.to_string() } else { format!("{lo} to {hi}") }, args.len()),
                            *pos,
                        );
                    }
                }
                for a in args {
                    self.check(a, used)?;
                }
            }
            Node::Bin { a, b, .. } => {
                self.check(a, used)?;
                self.check(b, used)?;
            }
            Node::Un { a, .. } => self.check(a, used)?,
            Node::Lit(_) => {}
        }
        Ok(())
    }
    fn eval_at(&self, node: &Node, i: usize, pre: &[Option<Vec<Val>>]) -> Val {
        match node {
            Node::Lit(v) => v.clone(),
            Node::Col { name, pos } => self.t.rows[i].get(self.lookup(name, *pos).unwrap_or(usize::MAX)).map_or(Val::Null, from_cell),
            Node::Un { op, a } => {
                let v = self.eval_at(a, i, pre);
                if *op == "!" {
                    Val::Bool(!truthy(&v))
                } else {
                    to_num(&v).map_or(Val::Null, |x| Val::Num(-x))
                }
            }
            Node::Bin { op, a, b } => {
                match *op {
                    "&&" => return Val::Bool(truthy(&self.eval_at(a, i, pre)) && truthy(&self.eval_at(b, i, pre))),
                    "||" => return Val::Bool(truthy(&self.eval_at(a, i, pre)) || truthy(&self.eval_at(b, i, pre))),
                    _ => {}
                }
                let (x, y) = (self.eval_at(a, i, pre), self.eval_at(b, i, pre));
                if matches!(*op, "==" | "!=" | "<" | "<=" | ">" | ">=") {
                    return Val::Bool(compare(&x, &y, op));
                }
                if *op == "+" && ((matches!(x, Val::Text(_)) && to_num(&x).is_none()) || (matches!(y, Val::Text(_)) && to_num(&y).is_none())) {
                    return Val::Text(text(&x) + &text(&y));
                }
                let (Some(p), Some(q)) = (to_num(&x), to_num(&y)) else { return Val::Null };
                match *op {
                    "+" => Val::Num(p + q),
                    "-" => Val::Num(p - q),
                    "*" => Val::Num(p * q),
                    "/" => {
                        if q == 0.0 {
                            Val::Null
                        } else {
                            Val::Num(p / q)
                        }
                    }
                    "%" => {
                        if q == 0.0 {
                            Val::Null
                        } else {
                            Val::Num(p - q * (p / q).floor())
                        }
                    }
                    _ => {
                        let r = p.powf(q);
                        if r.is_finite() {
                            Val::Num(r)
                        } else {
                            Val::Null
                        }
                    }
                }
            }
            Node::Call { name, args, id, .. } => {
                if is_group_fn(name) {
                    return pre[*id].as_ref().map_or(Val::Null, |v| v[i].clone());
                }
                if name == "if" || name == "ifelse" {
                    return if truthy(&self.eval_at(&args[0], i, pre)) { self.eval_at(&args[1], i, pre) } else { self.eval_at(&args[2], i, pre) };
                }
                if name == "coalesce" {
                    for a in args {
                        let v = self.eval_at(a, i, pre);
                        if !is_missing(&v) && v != Val::Text(String::new()) {
                            return v;
                        }
                    }
                    return Val::Null;
                }
                let vals: Vec<Val> = args.iter().map(|a| self.eval_at(a, i, pre)).collect();
                row_fn(name, &vals)
            }
        }
    }
    fn prepare(&self, node: &Node, pre: &mut Vec<Option<Vec<Val>>>, keys: Option<&[String]>) {
        match node {
            Node::Call { name, args, id, .. } => {
                for a in args {
                    self.prepare(a, pre, keys);
                }
                if is_group_fn(name) {
                    let n = self.t.rows.len();
                    let vec: Vec<Val> = if args.is_empty() { vec![Val::Num(0.0); n] } else { (0..n).map(|i| self.eval_at(&args[0], i, pre)).collect() };
                    let k = if args.len() > 1 && n > 0 { to_num(&self.eval_at(&args[1], 0, pre)) } else { None };
                    pre[*id] = Some(group_result(name, &vec, keys, k));
                }
            }
            Node::Bin { a, b, .. } => {
                self.prepare(a, pre, keys);
                self.prepare(b, pre, keys);
            }
            Node::Un { a, .. } => self.prepare(a, pre, keys),
            _ => {}
        }
    }
}

/// Evaluate an expression for every row of a table. `by` are the columns that define the groups for lag, first, cumsum, ….
pub fn evaluate(t: &Frame, src: &str, by: &[String]) -> Result<Evaluated, ExprError> {
    let (ast, ncalls) = parse(src)?;
    let ctx = Ctx {
        t,
        col_idx: t.columns.iter().enumerate().map(|(i, c)| (c.as_str(), i)).collect(),
        lower_idx: t.columns.iter().enumerate().rev().map(|(i, c)| (c.to_lowercase(), i)).collect(),
    };
    let mut used = Vec::new();
    let by_idx: Vec<usize> = by.iter().filter(|c| !c.is_empty()).map(|c| ctx.lookup(c, 0)).collect::<Result<_, _>>()?;
    let keys: Option<Vec<String>> =
        if by_idx.is_empty() { None } else { Some(t.rows.iter().map(|r| by_idx.iter().map(|j| r[*j].text()).collect::<Vec<_>>().join("\u{1}")).collect()) };
    ctx.check(&ast, &mut used)?;
    let mut pre: Vec<Option<Vec<Val>>> = vec![None; ncalls];
    ctx.prepare(&ast, &mut pre, keys.as_deref());
    let values = (0..t.rows.len()).map(|i| ctx.eval_at(&ast, i, &pre)).collect();
    Ok(Evaluated { values, columns: used })
}

/// Check that an expression parses and names existing columns, without evaluating it. Returns the error text, or empty.
pub fn validate(columns: &[String], src: &str, by: &[String]) -> String {
    let t = Frame { columns: columns.to_vec(), rows: vec![] };
    match evaluate(&t, src, by) {
        Ok(_) => String::new(),
        Err(e) => e.message,
    }
}
