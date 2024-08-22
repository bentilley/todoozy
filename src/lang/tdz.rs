// TODO Implement the .tdz file format +ideas
//
// I'm still not 100% sure we need this, but it feels like there is going to be a time where you
// have todos that you want to add but you don't know where they should live atm.
// ODOT
pub fn extract_todos(path: &str) -> Vec<(usize, usize, String)> {
    let filehandle = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(err) => {
            panic!("Error: {}", err);
        }
    };

    let _reader = std::io::BufReader::new(filehandle);

    // for line in reader.lines() {
    //     println!("{}", line.unwrap())
    // }

    Vec::new()
}
