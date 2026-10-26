fn main() {
    std::process::exit(xavier::cli::run(std::env::args().skip(1).collect()));
}
