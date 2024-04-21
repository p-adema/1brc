use std::fs::File;
use crate::buffers::init_buffers;
use crate::worker::{Parsers, read_worker, Station};

mod ref_hash_map;
mod worker;
mod buffers;


fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "measurements.txt".into());
    let file = File::open(path).expect("File is missing, please a correct path as the first argument");
    let threads: usize = std::thread::available_parallelism().unwrap().into();
    assert!(threads > 1, "This program expects to have at least two cores, and doesn't work single-threaded");
    let parse_threads = threads - 1;

    let (_owner, fill_handles, parse_handles) = unsafe { init_buffers(parse_threads) };
    let parsers = Parsers::start(parse_handles);

    read_worker(&fill_handles, file);

    show_results(parsers.join());
}

fn show_results(res: Vec<(String, Station)>) {
    print!("{{");
    for (i, (name, station)) in res.into_iter().enumerate() {
        if i != 0 {
            print!(", ");
        }
        print!("{name}={station}");
    }
    println!("}}");
}