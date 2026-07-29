use std::env;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match parse_socket_path().and_then(|path| gttyd::run(&path)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("gttyd: {error}");
            ExitCode::FAILURE
        }
    }
}

fn parse_socket_path() -> io::Result<PathBuf> {
    let mut arguments = env::args_os().skip(1);
    let mut socket_path = None;

    while let Some(argument) = arguments.next() {
        if argument == "--socket" {
            let path = arguments.next().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "--socket requires a path")
            })?;
            socket_path = Some(PathBuf::from(path));
        } else if argument == "--help" || argument == "-h" {
            println!("Usage: gttyd [--socket PATH]");
            std::process::exit(0);
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unknown argument: {}", argument.to_string_lossy()),
            ));
        }
    }

    socket_path.map_or_else(gttyd::default_socket_path, Ok)
}
