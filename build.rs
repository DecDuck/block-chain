const TEST: usize = 0;

use build_const::ConstWriter;

pub fn main() {
    let encryption = ConstWriter::for_build("encryption").expect("failed to make constants writer");
}