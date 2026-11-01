fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty()
        && std::env::args().next().is_some_and(|path| {
            std::path::Path::new(&path)
                .file_name()
                .is_some_and(|name| name == "kite")
        })
    {
        args.push("docs".into());
    }
    match kite_app::host_node::run(args) {
        Ok(code) => std::process::exit(code),
        Err(kite_app::host_node::Error::Io(error))
            if error.kind() == std::io::ErrorKind::BrokenPipe =>
        {
            std::process::exit(0)
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
