mod stations;

use std::io::Write;
use rand::prelude::*;
use stations::STATIONS;

const TOTAL: usize = 1_000_000_000;
const TEMPERATURE_STD: f64 = 10.;

fn main() {
    
    let mut out = std::io::BufWriter::new(std::fs::File::create_new("measurements.nosync").expect("Measurement file already exists!"));
    let mut station_rng = rand::rng();
    let mut temp_rng = rand::rng();

    for &(name, mean) in std::iter::from_fn(|| STATIONS.choose(&mut station_rng)).take(TOTAL) {
        let measurement: f64 = rand_distr::Normal::new(mean, TEMPERATURE_STD).unwrap().sample(&mut temp_rng);
        writeln!(out, "{name};{measurement:.1}").unwrap();
    };
}

