use std::io::{BufReader};
use std::io::prelude::*;
use std::fs;
use crossbeam_channel::{Sender, Receiver};
use std::thread::{self, JoinHandle};
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

fn parse_file_with_string(fd: std::fs::File, path: &str, substr: &str) -> Vec<String> {
    let header = [path, ":"].join("");
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

fn spawn_dir_walker_thread(tx_dirs: Sender<std::path::PathBuf>, rx_dirs: Receiver<std::path::PathBuf>, tx_files: Sender<(std::fs::File, std::path::PathBuf)>) -> JoinHandle<()>
{
    thread::spawn(move || {
        while let Ok(dir) = rx_dirs.try_recv() {
            if let Ok(rd) = fs::read_dir(dir.to_str().unwrap_or("")) {
                rd.filter(|de| de.is_ok())
                .map(|de| de.unwrap().path())
                //.filter(|path| filename_regex.is_match(path.to_str().unwrap_or("")))
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
    })
}

fn spawn_parser_thread(rx_parse: Receiver<(std::fs::File, std::path::PathBuf)>, substr: String, tx_output: Sender<Vec<String>>) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut parsed = 0;
        let start = Instant::now();
        while let Ok((file, path)) = rx_parse.recv() {
            let out = parse_file_with_string(file, path.to_str().unwrap_or(""), &substr);
            parsed += 1;
            if out.len() > 1 {
                match tx_output.send(out) {
                    Err(e) => println!("Error to send output to displayer: {:?}", e),
                    _ => continue,
                }
            }
        }
        let duration = start.elapsed();
        println!("Parsed {:?} files in {:?}.", parsed, duration);
    })
}

fn main() {
    let args = Args::parse();
    let concurrency_multiplier = args.concurrency_multiplier.unwrap_or(2);
    let num_parsers = num_cpus::get() * concurrency_multiplier;
    let _filename_regex = 
        match Regex::new(&args.filename_regex.unwrap_or(".*".to_string())) {
            Ok(v) => v,
            Err(e) => {
                println!("Error while parsing filename_regex: {:?}", e);
                std::process::exit(1)
            }
        };

    let start = Instant::now();

    let (tx_dirs, rx_dirs) = crossbeam_channel::unbounded();
    let (tx_files, rx_files) = crossbeam_channel::unbounded();
    let (tx_output, rx_output) = crossbeam_channel::unbounded();

    let init = std::path::PathBuf::from(".");
    if tx_dirs.send(init).is_err() {
        println!("Error initializing processing queues");
        std::process::exit(1);
    };

    let dir_walker = {
        let mut t = Vec::new();
        for _ in 0..num_parsers {
            t.push(spawn_dir_walker_thread(tx_dirs.clone(), rx_dirs.clone(), tx_files.clone()));
        }

        t
    };

    let substr = args.string.unwrap_or("".to_string());

    let parse_threads = {
        let mut t = Vec::new();
        for _ in 0..num_parsers {
            t.push(spawn_parser_thread(rx_files.clone(), substr.to_string(), tx_output.clone()));
        }

        t
    };

    let displayer = thread::spawn(move || {
        while let Ok(data) = rx_output.recv() {
            data.iter().for_each(|msg| println!("{}", msg));
        }
    });

    dir_walker
    .into_iter()
    .for_each(|h| {
        if h.join().is_err() {
            println!("Error while joining with directory traverser.");
        }
    });
    parse_threads
    .into_iter()
    .for_each(|h| {
        if h.join().is_err() {
            println!("Error while joining with parser.");
        }
    });

    if displayer.join().is_err() {
        println!("Error while joining with the output displayer");
    }
    
    let duration = start.elapsed();
    println!("Total time: {:?}", duration);
}