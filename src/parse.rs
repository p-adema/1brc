use std::fs::File;
use std::io::Read;
use std::sync::{Arc, Mutex};
use crate::worker::*;

pub(crate) const BLOCK_SIZE: usize = 50_000;
pub(crate) const N_BLOCKS: usize = 3;

pub(crate) type ThreadBuffer = Arc<[Mutex<[u8; BLOCK_SIZE]>; N_BLOCKS]>;
pub(crate) struct Buffers(Vec<ThreadBuffer>);

pub fn run(file: File) {
    let threads: usize = std::thread::available_parallelism().unwrap().into();
    assert!(threads > 1, "This program expects to have at least two cores, and doesn't work single-threaded");
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

fn read_worker(thread_buffers: Buffers, mut file: File) {
    let mut remainder = [0; 50];
    let mut remainder_size = 0;
    for thread_locks in thread_buffers.0.iter().cycle() {
        for mut buf in thread_locks.iter().filter_map(|l| l.try_lock().ok()).filter(|b| b[0] != 0) {
            let mut start = remainder_size;
            buf[..start].copy_from_slice(&remainder[..remainder_size]);
            loop {
                let read = file.read(&mut buf[start..]).unwrap();
                if read == 0 {
                    break;
                }
                start += read;
            }
            if start < BLOCK_SIZE {
                buf[start..].fill(0);
                return;
            }

            let last_nl = (BLOCK_SIZE - 50)
                + memchr::memrchr(b'\n', &buf[BLOCK_SIZE - 50..])
                .expect("Missing newline in file");

            let rem = &buf[last_nl + 1..];
            remainder_size = rem.len();
            remainder[..remainder_size].copy_from_slice(rem);
        }
    }
}

impl Buffers {
    fn new(parse_threads: usize) -> Self {
        Self(
            (0..parse_threads)
                .map(|_| {
                    Arc::new(
                        (0..N_BLOCKS)
                            .map(|_| Mutex::new([0; BLOCK_SIZE]))
                            .collect::<Vec<_>>()
                            .try_into()
                            .unwrap(),
                    )
                })
                .collect::<Vec<_>>()
        )
    }

    pub(crate) fn get(&self, nth: usize) -> ThreadBuffer {
        self.0[nth].clone()
    }
}


