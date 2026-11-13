use std::{
    io::{self, Write},
    process::exit,
};

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
        let input = buffer.as_str().trim();

        if input.starts_with(".") {
            match do_meta_command(input) {
                MetaCommandResult::MetaCommandSuccess => continue,
                MetaCommandResult::MetaCommmandUnrecognizedCommand => {
                    print!("Unrecognized command {}\n", input);
                    continue;
                }
            }
        }

        let statement = match prepare_statement(input) {
            Ok(state) => state,
            Err(_) => {
                println!("Unrecognized keyword at start of {input}.");
                continue;
            }
        };
        execute_statement(&statement);
        println!("Executed.");
    }
}

pub fn do_meta_command(input_buffer: &str) -> MetaCommandResult {
    if input_buffer.contains(".exit") {
        exit(0);
    } else {
        return MetaCommandResult::MetaCommmandUnrecognizedCommand;
    }
}

pub fn prepare_statement(input: &str) -> Result<Statement, PrepareResult> {
    if input.contains("insert") {
        return Ok(Statement {
            r#type: StatementType::Insert,
        });
    }
    if input.contains("select") {
        return Ok(Statement {
            r#type: StatementType::Select,
        });
    }
    return Err(PrepareResult::PrepareUnrecognizedStatement);
}

fn execute_statement(statement: &Statement) {
    match statement.r#type {
        StatementType::Insert => println!("This is where we would do an insert."),
        StatementType::Select => println!("This is where we would do a select."),
    }
}
pub enum MetaCommandResult {
    MetaCommandSuccess,
    MetaCommmandUnrecognizedCommand,
}

pub enum PrepareResult {
    PrepareUnrecognizedStatement,
}

pub enum StatementType {
    Insert,
    Select,
}

pub struct Statement {
    r#type: StatementType,
}
