use std::io::BufReader;
use std::io::prelude::*;
use std::fs::File;
use crossbeam_channel::{Sender, Receiver};
use std::thread::{self, JoinHandle};

use num_cpus;
use regex::Regex;
use clap::Parser;
use clap::ArgGroup;

use jwalk::*;

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
   regex: Option<String>,
   #[clap(short, long)]
   directory: Option<String>,
   #[clap(short, long)]
   matching_files_only: Option<bool>,
}

fn parse_dir_walker_thread(tx_files: Sender<DirEntry<((), ())>>, dir: String) -> JoinHandle<()> {
    thread::spawn(move || {
        for entry in WalkDirGeneric::<((), ())>::new(dir)
        .sort(false)
        .skip_hidden(true)
        .follow_links(false)
        {
            if let Ok(entry) = entry {
                if tx_files.clone().send(entry).is_err() {
                    break
                }
            }
        }

        drop(tx_files);
    })
}

fn does_file_match(fd: File, substr:& str) -> bool {
    BufReader::new(fd).lines()
        .take_while(|line| line.is_ok())
        .any(|line| {
            let txt = line.unwrap();
            txt.contains(substr)
        })
}

fn parse_file_with_string(fd: File, path: &str, substr: &str) -> Vec<String> {
    std::iter::once(path.to_string()).chain(
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

fn spawn_parser_thread(rx_parse: Receiver<jwalk::DirEntry<((), ())>>, substr: String, tx_output: Sender<Vec<String>>, matching_files_only: bool) -> JoinHandle<()> {
    thread::spawn(move || {
        while let Ok(entry) = rx_parse.recv() {
            let path = entry.path();
            if let Ok(file) = File::open(entry.path()) {
                if matching_files_only {
                    if does_file_match(file, &substr) {
                        match tx_output.send(vec![path.to_string_lossy().to_string()]) {
                            Err(e) => println!("Error to send output to displayer: {:?}", e),
                            _ => continue,
                        }
                    }
                } else {
                    let out = parse_file_with_string(file, path.to_str().unwrap_or(""), &substr);
                    if out.len() > 1 {
                        match tx_output.send(out) {
                            Err(e) => println!("Error to send output to displayer: {:?}", e),
                            _ => continue,
                        }
                    }
                }
            }
        }

        drop(tx_output);
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
    let directory = args.directory.unwrap_or(".".to_string());
    let matching_files_only = args.matching_files_only.unwrap_or(false);

    let (tx_files, rx_files) = crossbeam_channel::unbounded();
    let (tx_output, rx_output) = crossbeam_channel::unbounded();
    
    let dir_walker = parse_dir_walker_thread(tx_files, directory);

    let substr = args.string.unwrap_or("".to_string());

    let parse_threads = {
        let mut t = Vec::new();
        for _ in 0..num_parsers {
            t.push(spawn_parser_thread(rx_files.clone(), substr.to_string(), tx_output.clone(), matching_files_only));
        }

        t
    };
    drop(tx_output);

    let displayer = thread::spawn(move || {
        while let Ok(data) = rx_output.recv() {
            data.iter().for_each(|msg| println!("{}", msg));
        }
    });

    if dir_walker.join().is_err() {
        println!("Error while joining with dir walker");
    }

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
}