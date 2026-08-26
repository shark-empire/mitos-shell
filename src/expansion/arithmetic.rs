// src/expansion/arithmetic.rs

/// Evaluates $(( expr )) arithmetic. Returns the substituted string.
pub fn expand_arithmetic(input: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if i + 2 < chars.len() && chars[i] == '$' && chars[i + 1] == '(' && chars[i + 2] == '(' {
            if let Some((expr, end)) = extract_arithmetic(&chars, i + 3) {
                match evaluate(&expr) {
                    Ok(value) => result.push_str(&value.to_string()),
                    Err(_) => result.push_str("0"),
                }
                i = end;
                continue;
            }
        }
        result.push(chars[i]);
        i += 1;
    }

    result
}

fn extract_arithmetic(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut depth = 1;
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == '(' && chars[i + 1] == '(' {
            depth += 1;
            i += 2;
            continue;
        }
        if chars[i] == ')' && chars[i + 1] == ')' {
            depth -= 1;
            if depth == 0 {
                let inner: String = chars[start..i].iter().collect();
                return Some((inner, i + 2));
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    None
}

// ---------- Recursive-descent integer evaluator ----------

struct ArithParser {
    chars: Vec<char>,
    pos: usize,
}

pub fn evaluate(expr: &str) -> Result<i64, ()> {
    let mut p = ArithParser {
        chars: expr.chars().collect(),
        pos: 0,
    };
    let value = p.parse_expr()?;
    p.skip_ws();
    if p.pos != p.chars.len() {
        return Err(());
    }
    Ok(value)
}

impl ArithParser {
    fn skip_ws(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn parse_expr(&mut self) -> Result<i64, ()> {
        let mut value = self.parse_term()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some('+') => {
                    self.pos += 1;
                    value += self.parse_term()?;
                }
                Some('-') => {
                    self.pos += 1;
                    value -= self.parse_term()?;
                }
                _ => break,
            }
        }
        Ok(value)
    }

    fn parse_term(&mut self) -> Result<i64, ()> {
        let mut value = self.parse_factor()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some('*') => {
                    self.pos += 1;
                    value *= self.parse_factor()?;
                }
                Some('/') => {
                    self.pos += 1;
                    let rhs = self.parse_factor()?;
                    if rhs == 0 {
                        return Err(());
                    }
                    value /= rhs;
                }
                Some('%') => {
                    self.pos += 1;
                    let rhs = self.parse_factor()?;
                    if rhs == 0 {
                        return Err(());
                    }
                    value %= rhs;
                }
                _ => break,
            }
        }
        Ok(value)
    }

    fn parse_factor(&mut self) -> Result<i64, ()> {
        self.skip_ws();
        match self.peek() {
            Some('(') => {
                self.pos += 1;
                let value = self.parse_expr()?;
                self.skip_ws();
                if self.peek() == Some(')') {
                    self.pos += 1;
                    Ok(value)
                } else {
                    Err(())
                }
            }
            Some(c) if c.is_ascii_digit() => {
                let mut num_str = String::new();
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        num_str.push(c);
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                num_str.parse::<i64>().map_err(|_| ())
            }
            Some(c) if c.is_alphabetic() || c == '_' => {
                // Variable reference inside arithmetic
                let mut name = String::new();
                while let Some(c) = self.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        name.push(c);
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                let value = std::env::var(&name).unwrap_or_default();
                value.parse::<i64>().map_err(|_| ())
            }
            _ => Err(()),
        }
    }
}
