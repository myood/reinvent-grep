use std::io::{BufReader};
use std::io::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use num_cpus;
use regex::Regex;
use clap::Parser;
use clap::ArgGroup;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
#[clap(group(
    ArgGroup::new("lookup")
        .required(true)
        .args(&["string", "regex"]),
))]
struct Args {
   #[clap(short, long, value_parser)]
   concurrency_multiplier: Option<usize>,
   #[clap(short, long)]
   filename_regex: Option<String>,
   #[clap(short, long)]
   string: Option<String>,
   #[clap(short, long)]
   regex: Option<String>
}

fn parse_file_with_string(fd: std::fs::File, substr: &str) {
    let file = BufReader::new(fd);
    for line in file.lines() {
        match line {
            // TODO: Add proper results printing (more overhead)
            Ok(content) => { content.contains(substr); () }, 

            // TODO: Add proper error handling (?)
            //       Most likely file does not contain valid UTF-8 data
            Err(_) => return  
        }
    }
}

fn parse_file_with_regex(path: String, regex: &Regex) {
    match fs::read_to_string(&path) {
        Ok(_content) => {
            println!("{:?}", _content)
        },
        Err(_) => return,
    }
}

fn main() {
    let args = Args::parse();
    let concurrency_multiplier = args.concurrency_multiplier.unwrap_or(1);
    let num_parsers = num_cpus::get() * concurrency_multiplier;
    let filename_regex = 
        match Regex::new(&args.filename_regex.unwrap_or(".*".to_string())) {
            Ok(v) => v,
            Err(e) => {
                println!("Error while parsing filename_regex: {:?}", e);
                std::process::exit(1)
            }
        };

    let start = Instant::now();

    let (tx_dirs, rx_dirs) = mpsc::channel();
    let (tx_files, rx_files) = mpsc::channel();
    let get_parse_channels = || { 
        let mut rxs = Vec::new();
        let mut txs = Vec::new();
        println!("Spawning {:?} parsers", num_parsers);
        for _i in 0..num_parsers {
            let (tx, rx) = mpsc::channel();
            rxs.push(rx);
            txs.push(tx);
        }
        (txs, rxs)
    };
    let (tx_parse_channels, mut rx_parse_channels) = get_parse_channels();

    let init = std::path::PathBuf::from(".");
    if tx_dirs.send(init).is_err() {
        println!("Error initializing processing queues");
        std::process::exit(1);
    };

    let dir_walker = thread::spawn(move || {
        loop {
            // We are the only one pushing to the dirs channel (except initializer)
            // So if there is no dir on the queue, then there no more dirs to process
            let maybe_dir = rx_dirs.try_recv();
            match maybe_dir {
                Ok(dir) => {
                    let rd = fs::read_dir(dir.to_str().unwrap_or(""));
                    if rd.is_err() {
                        continue;
                    }
                    rd.unwrap()
                        .filter(|de| de.is_ok())
                        .map(|de| de.unwrap().path())
                        .filter(|path| filename_regex.is_match(path.to_str().unwrap_or("")))
                        .for_each(|path| {
                            let maybe_fd = std::fs::File::open(&path);
                            if maybe_fd.is_err() {
                                // It is likely a directory, or less likely permission denied
                                if tx_dirs.send(path).is_err() {
                                    println!("Error sending dir to dir walker");
                                }
                            }
                            else {
                                if tx_files.send(maybe_fd.unwrap()).is_err() {
                                    println!("Error sending file to parsers load balancer");
                                }
                            }
                        });
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
            let maybe_file = rx_files.recv();
            match maybe_file {
                Ok(file) => {
                    let tx_parse = &tx_parse_channels[it];
                    if tx_parse.send(file).is_err() {
                        println!("Error sending file to parser '{:?}'", it);
                        return
                    }
                    it = (it + 1) % tx_parse_channels.len();
                }
                Err(_) => {
                    // No more files to distribute across parsers
                    // tx_parse_channels implicitly dropped
                    return
                }
            }
        }
    });

    let substr = args.string.unwrap_or("".to_string());
    let mut get_parse_threads = || {
        let mut t = Vec::new();
        while rx_parse_channels.len() > 0 {
            let maybe_rx_parse = rx_parse_channels.pop();
            match maybe_rx_parse {
                Some(rx_parse) => {
                    let substr_copy = substr.to_string();
                    t.push(thread::spawn(move || {
                        let mut parsed = 0;
                        let start = Instant::now();
                        loop {
                            let maybe_file = rx_parse.recv();
                            match maybe_file {
                                Ok(file) => {
                                    parse_file_with_string(file, &substr_copy);
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

    
    let duration = start.elapsed();
    println!("Total time: {:?}", duration);
}