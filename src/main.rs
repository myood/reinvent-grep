use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

fn split_dirs(paths: Vec<PathBuf>) -> (Vec<String>, Vec<String>) {
    let dirs = paths.iter()
                .filter(|p| p.is_dir())
                .map(|ps| ps.to_str())
                .filter(|ps| ps.is_some())
                .map(|ps| ps.unwrap().to_string())
                .collect();
    let files = paths.iter()
                .filter(|p| !p.is_dir())
                .map(|ps| ps.to_str())
                .filter(|ps| ps.is_some())
                .map(|ps| ps.unwrap().to_string())
                .collect();
    (dirs, files)
}

fn list_dir(path: &str) -> Vec<PathBuf> {
    let rd = fs::read_dir(path);
    if rd.is_err() {
        println!("{:?} - {:?}", path, rd.unwrap_err());
        return Vec::new()
    }
    let rdi = rd.unwrap();
    rdi.filter(|de| de.is_ok())
        .map(|de| de.unwrap())
        .map(|de| de.path())
        .collect()
}

fn parse_file(path: String) {
    match fs::read_to_string(&path) {
        Ok(_content) => {
            
        },
        Err(_) => return,
    }
}

fn main() {
    let (tx_dirs, rx_dirs) = mpsc::channel();
    let (tx_files, rx_files) = mpsc::channel();
    let get_parse_channels = || { 
        let mut rxs = Vec::new();
        let mut txs = Vec::new();
        for _i in 0..10 {
            let (tx, rx) = mpsc::channel();
            rxs.push(rx);
            txs.push(tx);
        }
        (txs, rxs)
    };
    let (tx_parse_channels, mut rx_parse_channels) = get_parse_channels();

    let mut init = Vec::new();
    init.push(".".to_string());
    if tx_dirs.send(init).is_err() {
        println!("Error initializing processing queues");
        std::process::exit(1);
    };

    let dir_walker = thread::spawn(move || {
        loop {
            // We are the only one pushing to the dirs channel (except initializer)
            // So if there is no dir on the queue, then there no more dirs to process
            let maybe_dirs = rx_dirs.try_recv();
            match maybe_dirs {
                Ok(dirs) => {
                    for dir in dirs {

                        let entries = list_dir(&dir);
                        let (dirs, files) = split_dirs(entries);
                        if tx_dirs.send(dirs).is_err() {
                            println!("Error sending dirs");
                            return
                        }
                        if tx_files.send(files).is_err() {
                            println!("Error sending files");
                            return
                        }
                    }
                }
                Err(_) => {
                    // Notify file parser that no more files will be sent by closing the channel.
                    // All already sent files will be processed accordingly.
                    drop(tx_files);
                    return
                }
            }
        }
    });

    let load_balancer = thread::spawn(move || {
        let mut it = 0;
        loop {
            let maybe_files = rx_files.recv();
            match maybe_files {
                Ok(files) => {
                    for file in files {
                       let tx_parse = &tx_parse_channels[it];
                       if tx_parse.send(file).is_err() {
                           println!("Error sending file to parser '{:?}'", it);
                           return
                       }
                       it = (it + 1) % tx_parse_channels.len();
                    }
                }
                Err(_) => {
                    // No more files to distribute across parsers
                    // tx_parse_channels implicitly dropped
                    return
                }
            }
        }
    });

    let mut get_parse_threads = || {
        let mut t = Vec::new();
        while rx_parse_channels.len() > 0 {
            let maybe_rx_parse = rx_parse_channels.pop();
            match maybe_rx_parse {
                Some(rx_parse) => {
                    t.push(thread::spawn(move || {
                        let mut parsed = 0;
                        let start = Instant::now();
                        loop {
                            let maybe_file = rx_parse.recv();
                            match maybe_file {
                                Ok(file) => {
                                    parse_file(file);
                                    parsed += 1;
                                }
                                Err(_) => {
                                    let duration = start.elapsed();
                                    println!("Parsed {:?} files in {:?}.", parsed, duration);
                                    return
                                }
                            }
                        }
                    }))
                },
                None => {
                    println!("Internal error while spawning parsers.");
                }
            }
        }
        t
    };

    let parse_threads = get_parse_threads();

    if dir_walker.join().is_err() {
        println!("Error while joining with directory traverser.");
    }
    if load_balancer.join().is_err() {
        println!("Error while joining with load balancer.");
    }
    parse_threads
    .into_iter()
    .for_each(|h| {
        if h.join().is_err() {
            println!("Error while joining with parser.");
        }
    });
}