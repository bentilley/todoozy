pub fn extract_todos(path: &str) -> Vec<(u32, String)> {
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
