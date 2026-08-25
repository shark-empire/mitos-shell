use std::path::Path;
use std::fs;

/// Evaluates `test` or `[` conditions.
/// Returns 0 for true, 1 for false, 2 for syntax errors.
pub fn execute_test(args: &[String]) -> i32 {
    let is_bracket = args.first().map(|s| s.as_str()) == Some("[");
    let args = if is_bracket {
        if args.last().map(|s| s.as_str()) != Some("]") {
            eprintln!("[: missing ']'");
            return 2;
        }
        &args[1..args.len()-1] // Strip [ and ]
    } else {
        &args[1..] // Strip 'test'
    };

    if args.is_empty() { return 1; } // Empty condition is false

    // Handle unary operators (e.g., -f file, -z string)
    if args.len() == 2 {
        return match args[0].as_str() {
            "-f" => bool_to_status(Path::new(&args[1]).is_file()),
            "-d" => bool_to_status(Path::new(&args[1]).is_dir()),
            "-e" => bool_to_status(Path::new(&args[1]).exists()),
            "-r" => bool_to_status(Path::new(&args[1]).exists()), // Simplified read check
            "-w" => bool_to_status(Path::new(&args[1]).exists()), // Simplified write check
            "-x" => bool_to_status(Path::new(&args[1]).exists()), // Simplified exec check
            "-z" => bool_to_status(args[1].is_empty()),
            "-n" => bool_to_status(!args[1].is_empty()),
            _ => { eprintln!("test: unknown unary operator {}", args[0]); 2 }
        };
    }

    // Handle binary operators (e.g., str1 = str2, num1 -eq num2)
    if args.len() == 3 {
        let (left, op, right) = (&args[0], args[1].as_str(), &args[2]);
        return match op {
            "=" | "==" => bool_to_status(left == right),
            "!=" => bool_to_status(left != right),
            "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge" => {
                let l = left.parse::<i64>().unwrap_or(0);
                let r = right.parse::<i64>().unwrap_or(0);
                match op {
                    "-eq" => bool_to_status(l == r),
                    "-ne" => bool_to_status(l != r),
                    "-lt" => bool_to_status(l < r),
                    "-le" => bool_to_status(l <= r),
                    "-gt" => bool_to_status(l > r),
                    "-ge" => bool_to_status(l >= r),
                    _ => 2,
                }
            }
            _ => { eprintln!("test: unknown binary operator {}", op); 2 }
        };
    }

    // Handle single word (e.g., `test "string"`)
    if args.len() == 1 {
        return bool_to_status(!args[0].is_empty());
    }

    eprintln!("test: too many arguments or invalid syntax");
    2
}

fn bool_to_status(b: bool) -> i32 { if b { 0 } else { 1 } }
