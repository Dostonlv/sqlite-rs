use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};

use thiserror::Error;

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

#[derive(Debug, PartialEq)]
struct Row {
    id: u32,
    username: String,
    email: String,
}

impl Row {
    fn to_bytes(&self) -> [u8; ROW_SIZE] {
        let mut bytes = [0u8; ROW_SIZE];
        bytes[ID_OFFSET..ID_OFFSET + ID_SIZE].copy_from_slice(&self.id.to_le_bytes());
        bytes[USERNAME_OFFSET..USERNAME_OFFSET + self.username.len()]
            .copy_from_slice(self.username.as_bytes());
        bytes[EMAIL_OFFSET..EMAIL_OFFSET + self.email.len()].copy_from_slice(self.email.as_bytes());
        bytes
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        let id = u32::from_le_bytes(bytes[ID_OFFSET..ID_OFFSET + ID_SIZE].try_into().unwrap());
        let username = String::from_utf8_lossy(&bytes[USERNAME_OFFSET..EMAIL_OFFSET])
            .trim_end_matches('\0')
            .to_string();
        let email = String::from_utf8_lossy(&bytes[EMAIL_OFFSET..ROW_SIZE])
            .trim_end_matches('\0')
            .to_string();
        Row {
            id,
            username,
            email,
        }
    }
}

#[derive(Error, Debug)]
pub enum DbError {
    #[error("Syntax error. Could not parse statement.")]
    SyntaxError,

    #[error("String is too long.")]
    StringTooLong,

    #[error("ID must be positive.")]
    NegativeId,

    #[error("Error: Table full.")]
    TableFull,

    #[error("Unrecognized keyword at start of '{0}'.")]
    UnrecognizedStatement(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Tried to fetch page number out of bounds: {0}")]
    PageOutOfBounds(usize),
}

type Page = Box<[u8; PAGE_SIZE]>;

struct Pager {
    file: File,
    file_length: u64,
    pages: Vec<Option<Page>>,
}

impl Pager {
    fn open(path: impl AsRef<Path>) -> Result<Pager, DbError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        let file_length = file.metadata()?.len();
        Ok(Pager {
            file,
            file_length,
            pages: vec![None; TABLE_MAX_PAGES],
        })
    }

    fn get_page(&mut self, page_num: usize) -> Result<&mut Page, DbError> {
        if page_num >= TABLE_MAX_PAGES {
            return Err(DbError::PageOutOfBounds(page_num));
        }

        let slot = &mut self.pages[page_num];
        match slot {
            Some(page) => Ok(page),
            None => {
                let mut page: Page = Box::new([0u8; PAGE_SIZE]);
                let offset = (page_num * PAGE_SIZE) as u64;
                if offset < self.file_length {
                    let n = (self.file_length - offset).min(PAGE_SIZE as u64) as usize;
                    self.file.seek(SeekFrom::Start(offset))?;
                    self.file.read_exact(&mut page[..n])?;
                }
                Ok(slot.insert(page))
            }
        }
    }

    fn flush(&mut self, page_num: usize, size: usize) -> Result<(), DbError> {
        let Some(page) = &self.pages[page_num] else {
            return Ok(());
        };
        self.file
            .seek(SeekFrom::Start((page_num * PAGE_SIZE) as u64))?;
        self.file.write_all(&page[..size])?;
        Ok(())
    }
}

struct Table {
    num_rows: usize,
    pager: Pager,
}

impl Table {
    fn open(path: impl AsRef<Path>) -> Result<Table, DbError> {
        let pager = Pager::open(path)?;
        let len = pager.file_length as usize;
        let num_rows = (len / PAGE_SIZE) * ROWS_PER_PAGE + (len % PAGE_SIZE) / ROW_SIZE;
        Ok(Table { num_rows, pager })
    }
    fn locate(row_num: usize) -> (usize, usize) {
        let page_num = row_num / ROWS_PER_PAGE;
        let byte_offset = (row_num % ROWS_PER_PAGE) * ROW_SIZE;
        (page_num, byte_offset)
    }

    fn row_slot(&mut self, row_num: usize) -> Result<&mut [u8], DbError> {
        let (page_num, byte_offset) = Table::locate(row_num);
        let page = self.pager.get_page(page_num)?;
        Ok(&mut page[byte_offset..byte_offset + ROW_SIZE])
    }

    fn close(mut self) -> Result<(), DbError> {
        let full_pages = self.num_rows / ROWS_PER_PAGE;
        for i in 0..full_pages {
            self.pager.flush(i, PAGE_SIZE)?;
        }
        let rem = self.num_rows % ROWS_PER_PAGE;
        if rem > 0 {
            self.pager.flush(full_pages, rem * ROW_SIZE)?;
        }
        self.pager.file.sync_all()?;
        Ok(())
    }
    fn get_row(&mut self, row_num: usize) -> Result<Row, DbError> {
        Ok(Row::from_bytes(self.row_slot(row_num)?))
    }
}
fn main() -> Result<(), DbError> {
    println!(
        r#"SQLite version {VERSION}
Enter ".help" for usage hints.
Connected to a transient in-memory database.
Use ".open FILENAME" to reopen on a persistent database."#
    );

    let stdin = io::stdin();
    let mut buffer = String::new();
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("Must supply a database filename.");
        std::process::exit(1);
    };
    let mut table = Table::open(path)?;

    loop {
        print!("sqlite> ");
        io::stdout().flush()?;
        buffer.clear();

        if stdin.read_line(&mut buffer)? == 0 {
            break;
        }
        let input = buffer.trim();

        if input.starts_with(".") {
            match do_meta_command(input) {
                MetaCommandResult::UnrecognizedCommand => {
                    println!("Unrecognized command {}", input);
                    continue;
                }
                MetaCommandResult::Exit => break,
            }
        }

        let statement = match prepare_statement(input) {
            Ok(state) => state,
            Err(e) => {
                println!("{}", e);
                continue;
            }
        };
        match execute_statement(&statement, &mut table) {
            Ok(()) => println!("Executed."),
            Err(e) => println!("{}", e),
        }
    }
    table.close()
}

fn do_meta_command(input_buffer: &str) -> MetaCommandResult {
    match input_buffer {
        ".exit" => MetaCommandResult::Exit,
        _ => MetaCommandResult::UnrecognizedCommand,
    }
}

fn prepare_statement(input: &str) -> Result<Statement, DbError> {
    let mut parts = input.split_whitespace();
    match parts.next() {
        Some("insert") => {
            let token = parts.next().ok_or(DbError::SyntaxError)?;
            let id: i64 = token.parse().map_err(|_| DbError::SyntaxError)?;
            if id < 0 {
                return Err(DbError::NegativeId);
            }
            let id = u32::try_from(id).map_err(|_| DbError::SyntaxError)?;

            let username = parts.next().ok_or(DbError::SyntaxError)?.to_string();
            if username.len() > COLUMN_USERNAME_SIZE {
                return Err(DbError::StringTooLong);
            }
            let email = parts.next().ok_or(DbError::SyntaxError)?.to_string();
            if email.len() > COLUMN_EMAIL_SIZE {
                return Err(DbError::StringTooLong);
            }

            Ok(Statement::Insert(Row {
                id,
                username,
                email,
            }))
        }
        Some("select") => Ok(Statement::Select),
        _ => Err(DbError::UnrecognizedStatement(input.to_string())),
    }
}

fn execute_insert(row: &Row, table: &mut Table) -> Result<(), DbError> {
    if table.num_rows >= TABLE_MAX_ROWS {
        return Err(DbError::TableFull);
    }
    let bytes = row.to_bytes();
    table.row_slot(table.num_rows)?.copy_from_slice(&bytes);

    table.num_rows += 1;
    Ok(())
}

fn execute_select(table: &mut Table) -> Result<(), DbError> {
    for i in 0..table.num_rows {
        let row = table.get_row(i).unwrap();
        println!("({}, {}, {})", row.id, row.username, row.email)
    }
    Ok(())
}

fn execute_statement(statement: &Statement, table: &mut Table) -> Result<(), DbError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_retrieve() -> Result<(), DbError> {
        let (_dir, mut table) = test_table();
        execute_statement(
            &prepare_statement("insert 1 user1 person1@example.com")?,
            &mut table,
        )?;
        assert_eq!(table.num_rows, 1);
        assert_eq!(
            table.get_row(0)?,
            Row {
                id: 1,
                username: "user1".into(),
                email: "person1@example.com".into()
            }
        );
        Ok(())
    }

    #[test]
    fn test_table_full() {
        let (_dir, mut table) = test_table();
        for i in 0..TABLE_MAX_ROWS {
            let stmt = Statement::Insert(Row {
                id: i as u32,
                username: "tester".to_string(),
                email: "tester@example.com".to_string(),
            });
            assert!(execute_statement(&stmt, &mut table).is_ok());
        }
        let stmt = Statement::Insert(Row {
            id: 9999,
            username: "tester".to_string(),
            email: "tester@example.com".to_string(),
        });
        let result = execute_statement(&stmt, &mut table);
        assert!(matches!(result, Err(DbError::TableFull)));
    }

    #[test]
    fn test_string_too_long() {
        let long_username = "a".repeat(COLUMN_USERNAME_SIZE + 1);
        let input = format!("insert 1 {} email@example.com", long_username);
        let result = prepare_statement(&input);
        assert!(matches!(result, Err(DbError::StringTooLong)));
    }

    #[test]
    fn test_negative_id() {
        let result = prepare_statement("insert -1 user email");
        assert!(matches!(result, Err(DbError::NegativeId)));
    }

    fn test_table() -> (tempfile::TempDir, Table) {
        let dir = tempfile::tempdir().unwrap();
        let table = Table::open(dir.path().join("test.db")).unwrap();
        (dir, table)
    }

    #[test]
    fn keeps_data_after_close() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");

        let mut t = Table::open(&path).unwrap();
        let stmt = prepare_statement("insert 1 user1 person1@example.com").unwrap();
        execute_statement(&stmt, &mut t).unwrap();
        t.close().unwrap();

        let mut t = Table::open(&path).unwrap();
        assert_eq!(t.num_rows, 1);
        let row = t.get_row(0).unwrap();
        assert_eq!(row.username, "user1");
    }

    #[test]
    fn row_count_survives_many_pages() -> Result<(), DbError> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.db");
        let n = ROWS_PER_PAGE * 15 + 3;

        let mut t = Table::open(&path)?;
        for i in 0..n {
            let stmt = Statement::Insert(Row {
                id: i as u32,
                username: "u".into(),
                email: "e".into(),
            });
            execute_statement(&stmt, &mut t)?;
        }
        t.close()?;

        let t = Table::open(&path)?;
        assert_eq!(t.num_rows, n);
        Ok(())
    }
}
