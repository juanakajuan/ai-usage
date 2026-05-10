fn main() {
    if let Err(error) = ai_usage::application::run() {
        eprintln!("ai-usage: {error}");
        std::process::exit(1);
    }
}
