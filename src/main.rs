use std::io::{self, Write};

const VERSION: &str = "0.0.1";

fn main() -> io::Result<()> {
    println!(
        r#"SQLite version {VERSION}
Enter ".help" for usage hints.
Connected to a transient in-memory database.
Use ".open FILENAME" to reopen on a persistent database."#
    );

    loop {
        print!("sqlite> ");
        io::stdout().flush()?;

        let mut buffer = String::new();
        let stdin = io::stdin();
        stdin.read_line(&mut buffer)?;
        buffer.pop();
        let input = buffer.as_str().trim();
        match input {
            match input.strip_prefix(".") {

        }

            ".exit" => return Ok(()),
            _ => println!("Unrecognized command {buffer}."),
        }
    }
}

pub enum MetaCommandResult {
    MetaCommandSuccess,
    MetaCommmandUnrecognizedCommand,
}

pub enum PrepareResult {
    PrepareSuccess,
    PrepareUnrecognizedStatement,
}
