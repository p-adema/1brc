mod ref_hashmap;
mod parse;
mod worker;


fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "measurements.txt".into());
    let file = std::fs::File::open(path).expect("File is missing, please a correct path as the first argument");
    parse::run(file);
}
