use std::io::{self, Write};

const VERSION: &str = "0.0.1";
const COLUMN_USERNAME_SIZE: usize = 32;
const COLUMN_EMAIL_SIZE: usize = 255;
const ID_SIZE: usize = 4;
const ID_OFFSET: usize = 0;
const USERNAME_OFFSET: usize = ID_OFFSET + ID_SIZE;
const EMAIL_OFFSET: usize = USERNAME_OFFSET + COLUMN_USERNAME_SIZE;
const ROW_SIZE: usize = ID_SIZE + COLUMN_USERNAME_SIZE + COLUMN_EMAIL_SIZE;
const PAGE_SIZE: usize = 4096;
const TABLE_MAX_PAGES: usize = 100;
const ROWS_PER_PAGE: usize = PAGE_SIZE / ROW_SIZE;
const TABLE_MAX_ROWS: usize = TABLE_MAX_PAGES * ROWS_PER_PAGE;
struct Row {
    id: u32,
    username: [u8; COLUMN_USERNAME_SIZE],
    email: [u8; COLUMN_EMAIL_SIZE],
}

impl Row {
    fn serialize(&self, dest: &mut [u8]) {
        dest[ID_OFFSET..ID_OFFSET + ID_SIZE].copy_from_slice(&self.id.to_le_bytes());
        dest[USERNAME_OFFSET..EMAIL_OFFSET].copy_from_slice(&self.username);
        dest[EMAIL_OFFSET..ROW_SIZE].copy_from_slice(&self.email);
    }
    fn deserialize(src: &[u8]) -> Row {
        let id_bytes: [u8; ID_SIZE] = src[ID_OFFSET..ID_OFFSET + ID_SIZE]
            .try_into()
            .expect("id slice must be exactly ID_SIZE bytes");
        let id = u32::from_le_bytes(id_bytes);
        let mut username = [0u8; COLUMN_USERNAME_SIZE];
        username.copy_from_slice(&src[USERNAME_OFFSET..EMAIL_OFFSET]);
        let mut email = [0u8; COLUMN_EMAIL_SIZE];
        email.copy_from_slice(&src[EMAIL_OFFSET..ROW_SIZE]);
        Row {
            id,
            username,
            email,
        }
    }
}

type Page = Box<[u8; PAGE_SIZE]>;

struct Table {
    num_rows: usize,
    pages: Vec<Option<Page>>,
}

impl Table {
    fn new() -> Self {
        Table {
            num_rows: 0,
            pages: vec![None; TABLE_MAX_PAGES],
        }
    }
    fn locate(row_num: usize) -> (usize, usize) {
        let page_num = row_num / ROWS_PER_PAGE;
        let byte_offset = (row_num % ROWS_PER_PAGE) * ROW_SIZE;
        (page_num, byte_offset)
    }
    fn row_slot_mut(&mut self, row_num: usize) -> &mut [u8] {
        let (page_num, byte_offset) = Table::locate(row_num);
        // &mut self.pages[page_num].as_mut().unwrap()[byte_offset..byte_offset + ROW_SIZE]
        let page = self.pages[page_num].get_or_insert_with(|| Box::new([0u8; PAGE_SIZE]));
        &mut page[byte_offset..byte_offset + ROW_SIZE]
    }

    fn row_slot(&self, row_num: usize) -> &[u8] {
        let (page_num, byte_offset) = Table::locate(row_num);
        let page = self.pages[page_num]
            .as_ref()
            .expect("page must exist for a stored row");
        &page[byte_offset..byte_offset + ROW_SIZE]
    }
}
fn main() -> io::Result<()> {
    println!(
        r#"SQLite version {VERSION}
Enter ".help" for usage hints.
Connected to a transient in-memory database.
Use ".open FILENAME" to reopen on a persistent database."#
    );
    let stdin = io::stdin();
    let mut buffer = String::new();
    let mut table = Table::new();

    loop {
        print!("sqlite> ");
        io::stdout().flush()?;
        buffer.clear();

        if stdin.read_line(&mut buffer)? == 0 {
            break Ok(());
        }
        let input = buffer.trim();

        if input.starts_with(".") {
            match do_meta_command(input) {
                MetaCommandResult::UnrecognizedCommand => {
                    println!("Unrecognized command {}", input);
                    continue;
                }
                MetaCommandResult::Exit => break Ok(()),
            }
        }

        let statement = match prepare_statement(input) {
            Ok(state) => state,
            Err(PrepareError::SyntaxError) => {
                println!("Syntax error. Could not parse statement.");
                continue;
            }
            Err(PrepareError::UnrecognizedStatement) => {
                println!("Unrecognized keyword at start of '{input}'.");
                continue;
            }
            Err(PrepareError::StringTooLong) => {
                println!("String is too long.");
                continue;
            }
            Err(PrepareError::NegativeId) => {
                println!("ID must be positive.");
                continue;
            }
        };
        match execute_statement(&statement, &mut table) {
            Ok(()) => println!("Executed."),
            Err(ExecuteError::TableFull) => println!("Error: Table full."),
        }
    }
}

fn do_meta_command(input_buffer: &str) -> MetaCommandResult {
    match input_buffer {
        ".exit" => MetaCommandResult::Exit,
        _ => MetaCommandResult::UnrecognizedCommand,
    }
}

fn prepare_statement(input: &str) -> Result<Statement, PrepareError> {
    let mut parts = input.split_whitespace();
    match parts.next() {
        Some("insert") => {
            let token = parts.next().ok_or(PrepareError::SyntaxError)?;
            let id: i64 = token.parse().map_err(|_| PrepareError::SyntaxError)?;
            if id < 0 {
                return Err(PrepareError::NegativeId);
            }
            let id = u32::try_from(id).map_err(|_| PrepareError::SyntaxError)?;

            let username = parts.next().ok_or(PrepareError::SyntaxError)?;
            if username.len() > COLUMN_USERNAME_SIZE {
                return Err(PrepareError::StringTooLong);
            }
            let email = parts.next().ok_or(PrepareError::SyntaxError)?;
            if email.len() > COLUMN_EMAIL_SIZE {
                return Err(PrepareError::StringTooLong);
            }
            let mut row = Row {
                id,
                username: [0; COLUMN_USERNAME_SIZE],
                email: [0; COLUMN_EMAIL_SIZE],
            };
            row.username[..username.len()].copy_from_slice(username.as_bytes());
            row.email[..email.len()].copy_from_slice(email.as_bytes());
            Ok(Statement::Insert(row))
        }
        Some("select") => Ok(Statement::Select),
        _ => Err(PrepareError::UnrecognizedStatement),
    }
}

fn execute_insert(row: &Row, table: &mut Table) -> Result<(), ExecuteError> {
    if table.num_rows >= TABLE_MAX_ROWS {
        return Err(ExecuteError::TableFull);
    }
    row.serialize(table.row_slot_mut(table.num_rows));
    table.num_rows += 1;
    Ok(())
}

fn execute_select(table: &Table) -> Result<(), ExecuteError> {
    for i in 0..table.num_rows {
        let row = Row::deserialize(table.row_slot(i));
        println!(
            "({}, {}, {})",
            row.id,
            text_field(&row.username),
            text_field(&row.email)
        )
    }
    Ok(())
}

fn text_field(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end])

    // std::str::from_utf8(bytes)
    //     .unwrap_or("")
    //     .trim_matches('\0')
}
fn execute_statement(statement: &Statement, table: &mut Table) -> Result<(), ExecuteError> {
    match statement {
        Statement::Insert(row) => execute_insert(row, table),
        Statement::Select => execute_select(table),
    }
}
enum MetaCommandResult {
    Exit,
    UnrecognizedCommand,
}

#[allow(clippy::large_enum_variant)]
enum Statement {
    Insert(Row),
    Select,
}
enum PrepareError {
    SyntaxError,
    StringTooLong,
    NegativeId,
    UnrecognizedStatement,
}

enum ExecuteError {
    TableFull,
}
