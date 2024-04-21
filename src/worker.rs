use std::fmt::{Display, Formatter};
use std::sync::{Arc, mpsc, Mutex};
use std::thread::JoinHandle;
use std::fs::File;
use std::io::Read;

use crate::ref_hash_map::RefHashMap;

pub(crate) const BLOCK_SIZE: usize = 50_000;
pub(crate) const N_BLOCKS: usize = 3;

pub(crate) struct Buffers(Vec<ThreadBuffer>);

pub(crate) type ThreadBuffer = Arc<[Mutex<[u8; BLOCK_SIZE]>; N_BLOCKS]>;
pub(crate) type RefMap = RefHashMap<Vec<u8>, Station>;

fn parse_worker(
    stop_rx: mpsc::Receiver<()>,
    thread_buffer: ThreadBuffer,
) -> RefMap {
    let mut map: RefMap = RefHashMap::with_capacity(512);
    for i in (0..N_BLOCKS).cycle() {
        if i == 0 && stop_rx.try_recv().is_ok() {
            break;
        }

        let buf = thread_buffer[i].try_lock().ok();
        if buf.is_none() {
            continue;
        }
        let mut buf = buf.unwrap();
        if buf[0] == 0 {
            drop(buf);
            std::thread::sleep(std::time::Duration::new(0, 50));
            continue;
        }
        buf_parse(&mut map, buf.as_slice());
        buf[0] = 0;
    }
    // last pass
    for buf in (0..N_BLOCKS)
        .map(|i| thread_buffer[i].lock().unwrap())
        .filter(|buf| buf[0] != 0)
    {
        buf_parse(&mut map, buf.as_slice())
    }
    map
}

fn buf_parse(map: &mut RefMap, buf: &[u8]) {
    let mut start = 0;

    for pos in memchr::Memchr::new(b'\n', buf) {
        let bytes = &buf[start..pos];
        start = pos + 1;

        let line = Line::parse_bytes(bytes);
        update(map, line);
    }
}

pub struct Parsers {
    thread_handles: Vec<JoinHandle<RefMap>>,
    stop_handles: Vec<mpsc::SyncSender<()>>,
}

impl Parsers {
    pub(crate) fn start(parse_threads: usize, buffers: &Buffers) -> Self {
        let (thread_handles, stop_handles): (Vec<_>, Vec<_>) = (0..parse_threads)
            .map(|n| {
                let (stop_tx, stop_rx) = mpsc::sync_channel::<()>(1);
                let thread_buffer = buffers.get(n);
                let map_handle = std::thread::spawn(move || parse_worker(stop_rx, thread_buffer));

                (map_handle, stop_tx)
            })
            .unzip();
        Self { thread_handles, stop_handles }
    }

    pub(crate) fn join(self) -> Vec<(String, Station)> {
        self.stop_handles.into_iter().for_each(|s| s.send(()).unwrap());
        let mut res = self.thread_handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .fold(std::collections::HashMap::<String, Station>::with_capacity(512), |mut map, like| {
                for (name, station2) in like.into_iter() {
                    let name = String::from_utf8(name).expect("Should be valid UTF-8");
                    map.entry(name)
                        .and_modify(|station1| {
                            station1.update(&station2)
                        })
                        .or_insert(station2);
                }
                map
            })
            .into_iter()
            .collect::<Vec<(String, Station)>>();
        res.sort_unstable_by(|(n1, _), (n2, _)| n1.cmp(n2));
        res
    }
}


fn update(map: &mut RefMap, line: Line) {
    map.entry_ref(line.station)
        .and_modify(|station| {
            if line.measurement > station.max {
                station.max = line.measurement
            } else {
                station.min = station.min.min(line.measurement)
            }
            station.sum += line.measurement;
            station.count += 1;
        })
        .or_insert_with(|| Station {
            min: line.measurement,
            max: line.measurement,
            sum: line.measurement,
            count: 1,
        });
}

// min/max/sum 10x larger than true values
pub struct Station {
    min: i32,
    max: i32,
    sum: i32,
    count: u32,
}

impl Station {
    pub(crate) fn update(&mut self, other: &Self) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
        self.sum += other.sum;
        self.count += other.count;
    }
}

impl Display for Station {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:.1}/{:.1}/{:.1}",
            (self.min as f32) * 0.1,
            ((self.sum as f32) * 0.1) / (self.count as f32),
            (self.max as f32) * 0.1
        )
    }
}

struct Line<'a> {
    station: &'a [u8],
    measurement: i32,
}

impl<'a> Line<'a> {
    fn parse_bytes(s: &'a [u8]) -> Self {
        let colon_pos = memchr::memrchr(b';', s).unwrap();
        let fraction: i32 = (*s.last().unwrap() as char).to_digit(10).unwrap() as i32;
        let mut num = s[colon_pos + 2..s.len() - 2]
            .iter()
            .map(|d| (*d as char).to_digit(10).unwrap() as i32)
            .rev()
            .enumerate()
            .map(|(pow, num)| num * 10_i32.pow((pow + 1) as u32))
            .sum::<i32>()
            + fraction;

        let first_num_char = s[colon_pos + 1];
        if first_num_char == b'-' {
            num *= -1;
        } else {
            num += (first_num_char as char).to_digit(10).unwrap() as i32
                * 10_i32.pow((s.len() - colon_pos - 3) as u32)
        }
        Line {
            station: &s[..colon_pos],
            measurement: num,
        }
    }
}

pub fn read_worker(thread_buffers: Buffers, mut file: File) {
    let mut remainder = [0; 50];
    let mut remainder_size = 0;
    for thread_locks in thread_buffers.0.iter().cycle() {
        for mut buf in thread_locks.iter().filter_map(|l| l.try_lock().ok()).filter(|b| b[0] == 0) {
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
    pub(crate) fn new(parse_threads: usize) -> Self {
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
