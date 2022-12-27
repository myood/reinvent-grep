use std::io::BufReader;
use std::io::prelude::*;
use std::fs::File;
use crossbeam_channel::{Sender, Receiver};
use std::thread::{self, JoinHandle};
use std::time::Instant;

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
   regex: Option<String>
}

fn parse_dir_walker_thread(tx_files: Sender<DirEntry<((), Option<File>)>>) -> JoinHandle<()> {
    thread::spawn(move || {
        for entry in WalkDirGeneric::<((), Option<File>)>::new(".")
        .sort(false)
        .skip_hidden(true)
        .follow_links(false)
        .process_read_dir(|_, _, _, dir_entry_results| {
            dir_entry_results.iter_mut().for_each(|f| {
                if let Ok(entry) = f {
                    if entry.file_type().is_file() {
                        if let Ok(file) = File::open(entry.path()) {
                            entry.client_state = Some(file);
                        }
                    }
                }
            })
        }) 
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

fn parse_file_with_string(fd: File, path: &str, substr: &str) -> Vec<String> {
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

fn spawn_parser_thread(rx_parse: Receiver<jwalk::DirEntry<((), Option<File>)>>, substr: String, tx_output: Sender<Vec<String>>) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut parsed = 0;
        let start = Instant::now();
        while let Ok(entry) = rx_parse.recv() {
            let path = entry.path();
            if let Some(file) = entry.client_state {
                let out = parse_file_with_string(file, path.to_str().unwrap_or(""), &substr);
                parsed += 1;
                if out.len() > 1 {
                    match tx_output.send(out) {
                        Err(e) => println!("Error to send output to displayer: {:?}", e),
                        _ => continue,
                    }
                }
            }
        }
        let duration = start.elapsed();
        println!("Parsed {:?} files in {:?}.", parsed, duration);

        drop(tx_output);
    })
}

fn main() {
    let start = Instant::now();

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

    let (tx_files, rx_files) = crossbeam_channel::unbounded();
    let (tx_output, rx_output) = crossbeam_channel::unbounded();
    
    let dir_walker = parse_dir_walker_thread(tx_files);

    let substr = args.string.unwrap_or("".to_string());

    let parse_threads = {
        let mut t = Vec::new();
        for _ in 0..num_parsers {
            t.push(spawn_parser_thread(rx_files.clone(), substr.to_string(), tx_output.clone()));
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
    
    let duration = start.elapsed();
    println!("Total time: {:?}", duration);
}