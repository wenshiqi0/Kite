//! Linux native presentation only. All host policy is accessed through kite-node CLI.
mod native;
mod state;
fn main() {
    if let Err(error) = native::run() {
        eprintln!("Kite desktop: {error}");
        std::process::exit(1);
    }
}
