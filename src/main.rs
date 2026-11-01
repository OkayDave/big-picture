use clap::Parser;
fn main() -> std::process::ExitCode {
    let cli = big_picture::cli::Cli::parse();
    let json = cli.json;
    match big_picture::cli::run(cli) {
        Ok(code) => std::process::ExitCode::from(code as u8),
        Err(error) => {
            if json {
                println!("{}", serde_json::json!({"error":format!("{error:#}")}));
            } else {
                eprintln!("error: {error:#}");
            }
            std::process::ExitCode::FAILURE
        }
    }
}
