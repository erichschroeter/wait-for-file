use std::{path::{Path, PathBuf}, env, io, sync::{atomic::{AtomicBool, Ordering}, mpsc, Arc}, thread};

use clap::{command, value_parser, arg};
use path_clean::PathClean;


pub fn absolute_path(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    let path = path.as_ref();

    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    }.clean();

    Ok(absolute_path)
}

struct File(PathBuf);

fn wait_for_file(path: PathBuf, sender: mpsc::Sender<File>, is_file_found: Arc<AtomicBool>) {
    let path = absolute_path(&path).unwrap();
    log::info!("waiting for {} to exist", &path.display());
    match wait_file_created::robust_wait_read(&path) {
        Ok(_) => {
            is_file_found.store(true, Ordering::Relaxed);
            match sender.send(File { 0: path.clone() }) {
                Ok(_) => {},
                Err(_) => log::info!("Receiver has stopped listening for {}", path.display()),
            }
        },
        Err(e) => log::error!("Error waiting for file {}: {e}", &path.display()),
    }
}

fn main() -> Result<(), Box<std::io::Error>> {
    std::env::set_var("RUST_LOG", "actix_web=debug");
    std::env::set_var("RUST_LOG", "debug");
    // std::env::set_var("RUST_BACKTRACE", "1");
    env_logger::init();

    // requires `cargo` feature, reading name, version, author, and description from `Cargo.toml`
    let matches = command!()
        .arg(
            arg!(<file> ... "file")
                .value_parser(value_parser!(PathBuf))
                .help("The file paths to wait to exist."),
        )
        .get_matches();

    let is_file_found = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = mpsc::channel();

    let files = matches.get_many::<PathBuf>("file").unwrap().map(|p| p.to_owned());
    let mut handles = Vec::new();
    for path in files {
        let sender_n = sender.clone();
        let is_file_found = is_file_found.clone();
        let handle = thread::spawn(move || {
            wait_for_file(path, sender_n, is_file_found);
        });
        handles.push(handle);
    }

    let mut count_received_events = 0;
    while count_received_events < handles.len() {
        match receiver.recv() {
            Ok(File(p)) => {
                println!("{} exists!", p.display());
                count_received_events += 1;
            },
            Err(_) => panic!("Worker threads disconnected before file was found!"),
        }
    }

    for handle in handles {
        handle.join().unwrap();
    }
    log::info!("all files now exist");
    Ok(())
}
