use std::io::BufRead;

pub fn extract_todos(file_path: &str) -> Vec<(u32, String)> {
    let filehandle = match std::fs::File::open(file_path) {
        Ok(file) => file,
        Err(err) => {
            panic!("Error: {}", err);
        }
    };

    let line_comment_delimiter = format!("// {}", crate::constants::TODOOZY_DELIMITER);
    let block_comment_delimiter = format!("/* {}", crate::constants::TODOOZY_DELIMITER);

    /* TDZ (Z) 2024-08-06 Can it handle indented todos? +Testing
     *
     * This is a test todo with some indented lines:
     *   - This is an even more indented line.
     * ZDT */
    let reader = std::io::BufReader::new(filehandle);
    let mut lines = reader.lines();

    let mut todos = Vec::<(u32, String)>::new();

    // TDZ (A) 2024-08-09 This needs to work with single line todos. +Testing ZDT
    let mut line_number = 0;
    while let Some(line) = lines.next() {
        line_number += 1;
        match line {
            Ok(line) => {
                let mut todo: Vec<String> = Vec::new();
                if line.contains(&line_comment_delimiter) | line.contains(&block_comment_delimiter)
                {
                    let v: Vec<&str> = line.split(crate::constants::TODOOZY_DELIMITER).collect();
                    let txt = v[1].trim();

                    // Special case for single line todos.
                    if txt.contains(crate::constants::TODOOZY_DELIMITER_END) {
                        todo.push(
                            txt.replace(crate::constants::TODOOZY_DELIMITER_END, "")
                                .trim_end()
                                .to_owned(),
                        );
                        todos.push((line_number, todo.join("\n")));
                        continue;
                    }

                    // Otherwise, it's a multi-line todo, and we need to roll up all the lines
                    // until we hit the end delimiter.
                    let first_line_number = line_number;
                    let prefix = v[0].len();
                    todo.push(txt.to_owned());

                    while let Some(line) = lines.next() {
                        line_number += 1;
                        match line {
                            Ok(line) => {
                                if line.contains("ZDT") {
                                    let v = line.split("ZDT").collect::<Vec<&str>>();
                                    if v[0].len() > prefix {
                                        let txt = v[0][prefix..].trim_end();
                                        if txt.len() > 0 {
                                            todo.push(txt.to_owned());
                                        }
                                    }
                                    break;
                                } else {
                                    if line.len() < prefix {
                                        todo.push(String::from('\n'));
                                        continue;
                                    }
                                    todo.push(line[prefix..].trim_end().to_owned());
                                }
                            }
                            Err(err) => eprintln!("Error: {}", err),
                        }
                    }

                    todos.push((first_line_number, todo.join("\n")));
                }
            }
            Err(err) => eprintln!("Error: {}", err),
        }
    }

    todos
}
