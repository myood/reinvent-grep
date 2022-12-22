use std::io::{BufReader};
use std::io::prelude::*;
use std::fs;
use crossbeam_channel::unbounded;
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

fn parse_file_with_string(fd: std::fs::File, path: &std::path::PathBuf, substr: &str) -> Vec<String> {
    let header = [path.to_str().unwrap(), ":"].join("");
    std::iter::once(header).chain(
        BufReader::new(fd).lines()
            .take_while(|line| line.is_ok())
            .filter_map(|line| {
                let txt = line.unwrap();
                if txt.contains(substr) {
                    Some(txt)
                } else {
                    None
                }
            }))
        .collect::<Vec<String>>()
}

fn main() {
    let args = Args::parse();
    let concurrency_multiplier = args.concurrency_multiplier.unwrap_or(2);
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

    let (tx_dirs, rx_dirs) = unbounded();
    let (tx_files, rx_files) = unbounded();
    let (tx_output, rx_output) = unbounded();

    let init = std::path::PathBuf::from(".");
    if tx_dirs.send(init).is_err() {
        println!("Error initializing processing queues");
        std::process::exit(1);
    };

    let dir_walker = thread::spawn(move || {
        loop {
            // We are the only one pushing to the dirs channel (except initializer)
            // So if there is no dir on the queue, then there no more dirs to process
            if let Ok(dir) = rx_dirs.try_recv() {
                if let Ok(rd) = fs::read_dir(dir.to_str().unwrap_or("")) {
                    rd.filter(|de| de.is_ok())
                    .map(|de| de.unwrap().path())
                    .filter(|path| filename_regex.is_match(path.to_str().unwrap_or("")))
                    .for_each(|path| {
                        if let Ok(fd) = std::fs::File::open(&path) {
                            if tx_files.send( (fd, path) ).is_err() {
                                println!("Error sending file to parsers");
                            }
                        }
                        else {// It is likely a directory, or less likely permission denied
                            if tx_dirs.send(path).is_err() {
                                println!("Error sending dir to dir walker");
                            }
                        }
                    });
                }
            }
            else {
                // Notify file parser that no more files will be sent by closing the channel.
                // All already sent files will be processed accordingly.
                drop(tx_files);
                return
            }
        }
    });

    let substr = args.string.unwrap_or("".to_string());
    let get_parse_threads = || {
        let mut t = Vec::new();
        for _ in 0..num_parsers {
            let rx_parse = rx_files.clone();
            let substr_copy = substr.to_string();
            let tx_output_copy = tx_output.clone();
            t.push(thread::spawn(move || {
                let mut parsed = 0;
                let start = Instant::now();
                loop {
                    let maybe_file = rx_parse.recv();
                    match maybe_file {
                        Ok( (file, path) ) => {
                            let out = parse_file_with_string(file, &path, &substr_copy);
                            parsed += 1;
                            if out.len() > 1 {
                                match tx_output_copy.send(out) {
                                    Err(e) => println!("Error to send output to displayer: {:?}", e),
                                    _ => continue,
                                }
                            }
                        }
                        Err(_) => {
                            let duration = start.elapsed();
                            println!("Parsed {:?} files in {:?}.", parsed, duration);
                            return
                        }
                    }
                }
            }))

        }
        t
    };

    let parse_threads = get_parse_threads();

    let displayer = thread::spawn(move || {
        loop {
            match rx_output.recv() {
                Ok(data) => data.iter().for_each(|msg| println!("{}", msg)),
                Err(_) => { return },
            }
        }
    });

    if dir_walker.join().is_err() {
        println!("Error while joining with directory traverser.");
    }
    parse_threads
    .into_iter()
    .for_each(|h| {
        if h.join().is_err() {
            println!("Error while joining with parser.");
        }
    });

    drop(tx_output);

    if displayer.join().is_err() {
        println!("Error while joining with the output displayer");
    }
    
    let duration = start.elapsed();
    println!("Total time: {:?}", duration);
}