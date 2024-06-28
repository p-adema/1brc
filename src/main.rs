use parse::worker::{read_worker, Buffers, Parsers, Station};
use std::fs::File;


fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "measurements.txt".into());
    let file =
        File::open(path).expect("File is missing, please a correct path as the first argument");
    let threads: usize = std::thread::available_parallelism().unwrap().into();
    assert!(
        threads > 1,
        "This program expects to have at least two cores, and doesn't work single-threaded"
    );
    let parse_threads = threads - 1;

    let buffers: Buffers = Buffers::new(parse_threads);
    let parsers = Parsers::start(parse_threads, &buffers);

    read_worker(buffers, file);

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
