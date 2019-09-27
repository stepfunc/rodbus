
#[derive(Debug)]
pub enum Error {
    IO(std::io::Error),
    BadADUSize
}