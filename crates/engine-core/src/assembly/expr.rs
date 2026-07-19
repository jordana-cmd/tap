//! A small, safe, deterministic expression engine for assembly formulas.
//!
//! Formulas are TOKENIZED → parsed to an AST → evaluated against a variable
//! context. There is no `eval`, no arbitrary code, and no host access — just
//! arithmetic, comparisons, a fixed function set, and `if`. Every failure is
//! a typed [`ExprError`]; **no path returns a silent 0 or NaN** — a bad
//! formula never reaches a quantity as a plausible-looking number.

use std::collections::BTreeMap;

/// A parse or evaluation failure. Callers surface these with the offending
/// part so an estimator can see exactly why a number could not be produced.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ExprError {
    #[error("syntax error: {0}")]
    Syntax(String),
    #[error("unknown variable `{0}`")]
    UnknownVariable(String),
    #[error("unknown function `{0}`")]
    UnknownFunction(String),
    #[error("division by zero")]
    DivideByZero,
    #[error("type error: {0}")]
    TypeError(String),
    #[error("`{func}` expects {expected} argument(s), got {got}")]
    ArgCount {
        func: String,
        expected: String,
        got: usize,
    },
}

/// A runtime value: assembly math is numeric, but comparisons yield a bool
/// consumed only by `if`.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Value {
    Num(f64),
    Bool(bool),
}

impl Value {
    fn as_num(self, ctx: &str) -> Result<f64, ExprError> {
        match self {
            Value::Num(n) => Ok(n),
            Value::Bool(_) => Err(ExprError::TypeError(format!("{ctx} expects a number, got a boolean"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Func {
    Ceil,
    Floor,
    Round,
    Abs,
    Min,
    Max,
    If,
}

impl Func {
    fn from_name(name: &str) -> Option<Func> {
        Some(match name {
            "ceil" => Func::Ceil,
            "floor" => Func::Floor,
            "round" => Func::Round,
            "abs" => Func::Abs,
            "min" => Func::Min,
            "max" => Func::Max,
            "if" => Func::If,
            _ => return None,
        })
    }
    fn name(self) -> &'static str {
        match self {
            Func::Ceil => "ceil",
            Func::Floor => "floor",
            Func::Round => "round",
            Func::Abs => "abs",
            Func::Min => "min",
            Func::Max => "max",
            Func::If => "if",
        }
    }
}

/// A parsed formula — an OPAQUE handle over the internal AST. Cheap to
/// evaluate repeatedly; parse once, eval per apply. Consumers see only
/// [`Expr::eval_num`]; the node shapes are engine-internal.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr(Node);

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Num(f64),
    Var(String),
    Neg(Box<Node>),
    Binary(BinOp, Box<Node>, Box<Node>),
    Call(Func, Vec<Node>),
}

// ---------- tokenizer ----------

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
    Comma,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

fn tokenize(src: &str) -> Result<Vec<Token>, ExprError> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' | '\r' | '\n' => i += 1,
            '+' => { out.push(Token::Plus); i += 1; }
            '-' => { out.push(Token::Minus); i += 1; }
            '*' => { out.push(Token::Star); i += 1; }
            '/' => { out.push(Token::Slash); i += 1; }
            '(' => { out.push(Token::LParen); i += 1; }
            ')' => { out.push(Token::RParen); i += 1; }
            ',' => { out.push(Token::Comma); i += 1; }
            '<' => {
                if chars.get(i + 1) == Some(&'=') { out.push(Token::Le); i += 2; }
                else { out.push(Token::Lt); i += 1; }
            }
            '>' => {
                if chars.get(i + 1) == Some(&'=') { out.push(Token::Ge); i += 2; }
                else { out.push(Token::Gt); i += 1; }
            }
            '=' => {
                if chars.get(i + 1) == Some(&'=') { out.push(Token::Eq); i += 2; }
                else { return Err(ExprError::Syntax("`=` must be `==`".into())); }
            }
            '!' => {
                if chars.get(i + 1) == Some(&'=') { out.push(Token::Ne); i += 2; }
                else { return Err(ExprError::Syntax("`!` must be `!=`".into())); }
            }
            c if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                let n: f64 = s
                    .parse()
                    .map_err(|_| ExprError::Syntax(format!("invalid number `{s}`")))?;
                out.push(Token::Num(n));
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                out.push(Token::Ident(chars[start..i].iter().collect()));
            }
            other => return Err(ExprError::Syntax(format!("unexpected character `{other}`"))),
        }
    }
    Ok(out)
}

// ---------- recursive-descent parser ----------

struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.toks.get(self.pos)
    }
    fn advance(&mut self) -> Option<Token> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }
    fn expect(&mut self, want: &Token, what: &str) -> Result<(), ExprError> {
        match self.advance() {
            Some(ref t) if t == want => Ok(()),
            other => Err(ExprError::Syntax(format!("expected {what}, found {other:?}"))),
        }
    }

    /// expr → comparison
    fn expr(&mut self) -> Result<Node, ExprError> {
        let lhs = self.add_sub()?;
        let op = match self.peek() {
            Some(Token::Lt) => BinOp::Lt,
            Some(Token::Le) => BinOp::Le,
            Some(Token::Gt) => BinOp::Gt,
            Some(Token::Ge) => BinOp::Ge,
            Some(Token::Eq) => BinOp::Eq,
            Some(Token::Ne) => BinOp::Ne,
            _ => return Ok(lhs),
        };
        self.pos += 1;
        let rhs = self.add_sub()?; // non-associative: one comparison per expr
        Ok(Node::Binary(op, Box::new(lhs), Box::new(rhs)))
    }

    fn add_sub(&mut self) -> Result<Node, ExprError> {
        let mut node = self.mul_div()?;
        loop {
            let op = match self.peek() {
                Some(Token::Plus) => BinOp::Add,
                Some(Token::Minus) => BinOp::Sub,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.mul_div()?;
            node = Node::Binary(op, Box::new(node), Box::new(rhs));
        }
        Ok(node)
    }

    fn mul_div(&mut self) -> Result<Node, ExprError> {
        let mut node = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(Token::Star) => BinOp::Mul,
                Some(Token::Slash) => BinOp::Div,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.unary()?;
            node = Node::Binary(op, Box::new(node), Box::new(rhs));
        }
        Ok(node)
    }

    fn unary(&mut self) -> Result<Node, ExprError> {
        if self.peek() == Some(&Token::Minus) {
            self.pos += 1;
            return Ok(Node::Neg(Box::new(self.unary()?)));
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Node, ExprError> {
        match self.advance() {
            Some(Token::Num(n)) => Ok(Node::Num(n)),
            Some(Token::LParen) => {
                let e = self.expr()?;
                self.expect(&Token::RParen, "`)`")?;
                Ok(e)
            }
            Some(Token::Ident(name)) => {
                if self.peek() == Some(&Token::LParen) {
                    // Function call.
                    let func = Func::from_name(&name)
                        .ok_or_else(|| ExprError::UnknownFunction(name.clone()))?;
                    self.pos += 1; // consume '('
                    let mut args = Vec::new();
                    if self.peek() != Some(&Token::RParen) {
                        loop {
                            args.push(self.expr()?);
                            match self.peek() {
                                Some(Token::Comma) => { self.pos += 1; }
                                _ => break,
                            }
                        }
                    }
                    self.expect(&Token::RParen, "`)`")?;
                    Ok(Node::Call(func, args))
                } else {
                    Ok(Node::Var(name))
                }
            }
            other => Err(ExprError::Syntax(format!("unexpected token {other:?}"))),
        }
    }
}

/// Parse a formula string into an [`Expr`]. Trailing tokens are a syntax error.
pub fn parse(src: &str) -> Result<Expr, ExprError> {
    let toks = tokenize(src)?;
    if toks.is_empty() {
        return Err(ExprError::Syntax("empty formula".into()));
    }
    let mut p = Parser { toks, pos: 0 };
    let e = p.expr()?;
    if p.pos != p.toks.len() {
        return Err(ExprError::Syntax(format!(
            "unexpected trailing token {:?}",
            p.toks[p.pos]
        )));
    }
    Ok(Expr(e))
}

impl Expr {
    /// Evaluate to a number against `vars`. A boolean-valued top-level
    /// expression is a type error (formulas produce quantities, not flags).
    pub fn eval_num(&self, vars: &BTreeMap<String, f64>) -> Result<f64, ExprError> {
        self.0.eval(vars)?.as_num("formula")
    }
}

impl Node {
    fn eval(&self, vars: &BTreeMap<String, f64>) -> Result<Value, ExprError> {
        match self {
            Node::Num(n) => Ok(Value::Num(*n)),
            Node::Var(name) => vars
                .get(name)
                .copied()
                .map(Value::Num)
                .ok_or_else(|| ExprError::UnknownVariable(name.clone())),
            Node::Neg(e) => Ok(Value::Num(-e.eval(vars)?.as_num("negation")?)),
            Node::Binary(op, a, b) => Self::eval_binary(*op, a, b, vars),
            Node::Call(func, args) => Self::eval_call(*func, args, vars),
        }
    }

    fn eval_binary(
        op: BinOp,
        a: &Node,
        b: &Node,
        vars: &BTreeMap<String, f64>,
    ) -> Result<Value, ExprError> {
        let x = a.eval(vars)?.as_num("operand")?;
        let y = b.eval(vars)?.as_num("operand")?;
        Ok(match op {
            BinOp::Add => Value::Num(x + y),
            BinOp::Sub => Value::Num(x - y),
            BinOp::Mul => Value::Num(x * y),
            BinOp::Div => {
                if y == 0.0 {
                    return Err(ExprError::DivideByZero);
                }
                Value::Num(x / y)
            }
            BinOp::Lt => Value::Bool(x < y),
            BinOp::Le => Value::Bool(x <= y),
            BinOp::Gt => Value::Bool(x > y),
            BinOp::Ge => Value::Bool(x >= y),
            BinOp::Eq => Value::Bool(x == y),
            BinOp::Ne => Value::Bool(x != y),
        })
    }

    fn eval_call(
        func: Func,
        args: &[Node],
        vars: &BTreeMap<String, f64>,
    ) -> Result<Value, ExprError> {
        let arity_err = |expected: &str| ExprError::ArgCount {
            func: func.name().to_string(),
            expected: expected.to_string(),
            got: args.len(),
        };
        match func {
            Func::If => {
                if args.len() != 3 {
                    return Err(arity_err("3"));
                }
                let cond = match args[0].eval(vars)? {
                    Value::Bool(b) => b,
                    Value::Num(_) => {
                        return Err(ExprError::TypeError(
                            "`if` condition must be a comparison".into(),
                        ))
                    }
                };
                // Lazily evaluate only the taken branch: `if(x>0, 1/x, 0)`
                // must not divide by zero when x <= 0.
                args[if cond { 1 } else { 2 }].eval(vars)
            }
            Func::Ceil | Func::Floor | Func::Round | Func::Abs => {
                if args.len() != 1 {
                    return Err(arity_err("1"));
                }
                let v = args[0].eval(vars)?.as_num(func.name())?;
                Ok(Value::Num(match func {
                    Func::Ceil => v.ceil(),
                    Func::Floor => v.floor(),
                    Func::Round => v.round(), // half away from zero
                    Func::Abs => v.abs(),
                    _ => unreachable!(),
                }))
            }
            Func::Min | Func::Max => {
                if args.len() < 2 {
                    return Err(arity_err("at least 2"));
                }
                let mut acc = args[0].eval(vars)?.as_num(func.name())?;
                for a in &args[1..] {
                    let v = a.eval(vars)?.as_num(func.name())?;
                    acc = if func == Func::Min { acc.min(v) } else { acc.max(v) };
                }
                Ok(Value::Num(acc))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }
    fn eval(src: &str, v: &[(&str, f64)]) -> Result<f64, ExprError> {
        parse(src)?.eval_num(&vars(v))
    }

    #[test]
    fn literals_precedence_parens_unary() {
        assert_eq!(eval("1 + 2 * 3", &[]).unwrap(), 7.0);
        assert_eq!(eval("(1 + 2) * 3", &[]).unwrap(), 9.0);
        assert_eq!(eval("10 / 2 / 5", &[]).unwrap(), 1.0); // left-assoc
        assert_eq!(eval("-3 + 1", &[]).unwrap(), -2.0);
        assert_eq!(eval("2 * -4", &[]).unwrap(), -8.0);
        assert_eq!(eval("3.5 * 2", &[]).unwrap(), 7.0);
    }

    #[test]
    fn variables_bound_and_unbound() {
        assert_eq!(eval("area_sf / 20", &[("area_sf", 2475.0)]).unwrap(), 123.75);
        assert_eq!(
            eval("area_sf / 20", &[]).unwrap_err(),
            ExprError::UnknownVariable("area_sf".into())
        );
    }

    #[test]
    fn functions_and_arity() {
        assert_eq!(eval("ceil(1.1)", &[]).unwrap(), 2.0);
        assert_eq!(eval("floor(1.9)", &[]).unwrap(), 1.0);
        assert_eq!(eval("round(1.5)", &[]).unwrap(), 2.0);
        assert_eq!(eval("round(2.5)", &[]).unwrap(), 3.0); // half away from zero
        assert_eq!(eval("abs(-4.2)", &[]).unwrap(), 4.2);
        assert_eq!(eval("min(3, 5, 1)", &[]).unwrap(), 1.0);
        assert_eq!(eval("max(3, 5, 1)", &[]).unwrap(), 5.0);
        assert!(matches!(
            eval("ceil(1, 2)", &[]).unwrap_err(),
            ExprError::ArgCount { .. }
        ));
        assert!(matches!(
            eval("min(3)", &[]).unwrap_err(),
            ExprError::ArgCount { .. }
        ));
    }

    #[test]
    fn conditionals_every_comparison() {
        assert_eq!(eval("if(1 < 2, 10, 20)", &[]).unwrap(), 10.0);
        assert_eq!(eval("if(2 <= 2, 10, 20)", &[]).unwrap(), 10.0);
        assert_eq!(eval("if(3 > 5, 10, 20)", &[]).unwrap(), 20.0);
        assert_eq!(eval("if(5 >= 5, 10, 20)", &[]).unwrap(), 10.0);
        assert_eq!(eval("if(4 == 4, 10, 20)", &[]).unwrap(), 10.0);
        assert_eq!(eval("if(4 != 4, 10, 20)", &[]).unwrap(), 20.0);
    }

    #[test]
    fn if_branch_is_lazy() {
        // The untaken branch must not be evaluated (no div-by-zero at x=0).
        assert_eq!(eval("if(x > 0, 1 / x, 0)", &[("x", 0.0)]).unwrap(), 0.0);
        assert_eq!(eval("if(x > 0, 1 / x, 0)", &[("x", 4.0)]).unwrap(), 0.25);
    }

    #[test]
    fn error_paths() {
        assert_eq!(eval("1 / 0", &[]).unwrap_err(), ExprError::DivideByZero);
        assert_eq!(eval("area / 0", &[("area", 5.0)]).unwrap_err(), ExprError::DivideByZero);
        assert!(matches!(eval("foo(2)", &[]).unwrap_err(), ExprError::UnknownFunction(_)));
        assert!(matches!(eval("2 +", &[]).unwrap_err(), ExprError::Syntax(_)));
        assert!(matches!(eval("2 3", &[]).unwrap_err(), ExprError::Syntax(_)));
        assert!(matches!(eval("", &[]).unwrap_err(), ExprError::Syntax(_)));
        assert!(matches!(eval("1 & 2", &[]).unwrap_err(), ExprError::Syntax(_)));
        // A bare comparison at top level → boolean → type error.
        assert!(matches!(eval("1 < 2", &[]).unwrap_err(), ExprError::TypeError(_)));
        // Arithmetic on a boolean.
        assert!(matches!(eval("(1 < 2) + 1", &[]).unwrap_err(), ExprError::TypeError(_)));
        // `if` condition that is a number, not a comparison.
        assert!(matches!(eval("if(1, 2, 3)", &[]).unwrap_err(), ExprError::TypeError(_)));
    }

    #[test]
    fn no_silent_nan_or_zero() {
        // Every failure is an Err — never Ok(0.0)/Ok(NaN).
        for bad in ["1/0", "unbound", "ceil()", "foo(1)", "1<2"] {
            assert!(eval(bad, &[]).is_err(), "`{bad}` should be an error");
        }
    }
}
