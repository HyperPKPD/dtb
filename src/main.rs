use clap::Parser;
use dtb::cli::{error_json, rewrite_groups, run, Cli};

fn main() {
    let args = rewrite_groups(std::env::args().collect());
    let json_errors = args.iter().any(|a| a == "--json-errors");
    let positional: Vec<&String> = args.iter().skip(1).filter(|a| !a.starts_with('-')).collect();
    let command = match positional.first().map(|s| s.as_str()) {
        Some("call") => positional.get(1).map(|s| s.to_string()).unwrap_or_default(),
        Some(c) => c.to_string(),
        None => String::new(),
    };
    let cli = match Cli::try_parse_from(&args) {
        Ok(c) => c,
        Err(e) => {
            let code = if e.use_stderr() { 2 } else { 0 };
            if json_errors && code != 0 {
                println!(
                    "{}",
                    error_json(
                        &command,
                        &dtb::core::error::DtbError::Usage(e.to_string().lines().next().unwrap_or("").trim_start_matches("error: ").to_string())
                    )
                );
            } else {
                let _ = e.print();
            }
            std::process::exit(code);
        }
    };
    let quiet = cli.quiet;
    // run on a thread with a large stack: the expression and JSON parsers recurse
    let handle = std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(move || run(cli));
    let result = match handle.map(|h| h.join()) {
        Ok(Ok(r)) => r,
        Ok(Err(_)) => Err(dtb::core::error::DtbError::Internal("a command panicked (this is a bug in dtb)".into())),
        Err(e) => Err(dtb::core::error::DtbError::Internal(format!("cannot start: {e}"))),
    };
    if let Err(e) = result {
        if json_errors {
            println!("{}", error_json(&command, &e));
        } else if !quiet || e.exit_code() == 70 {
            eprintln!("dtb: error: {e}");
        }
        std::process::exit(e.exit_code());
    }
}
