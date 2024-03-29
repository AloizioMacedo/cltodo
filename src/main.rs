use chrono::{DateTime, Local, NaiveDate, ParseError};
use home::home_dir;
use sqlx::{query, sqlite::SqlitePoolOptions, FromRow, Pool, QueryBuilder, Sqlite};
use std::io::{self, Write};
use std::path::PathBuf;
use std::{
    fs::{create_dir_all, OpenOptions},
    process::Command,
    str::FromStr,
    time,
};

use clap::{Parser, Subcommand, ValueEnum};
use colored::Colorize;

const DB_FOLDER: &str = ".cltodo";
const DB_FILE: &str = "data.db";

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let args = Cli::parse();

    let global = args.global;

    let pool = get_connection(global).await?;

    let query = sqlx::query!(
        "CREATE TABLE IF NOT EXISTS todos (
            id INTEGER PRIMARY KEY,
            date TEXT NOT NULL,
            text TEXT NOT NULL,
            priority INTEGER NOT NULL
        ) STRICT"
    );
    query.execute(&pool).await?;

    match args.command {
        Commands::Add { text, priority } => post_todo(&text, &pool, priority).await?,
        Commands::Get {
            priority,
            from,
            to,
            reversed,
            extended,
            chronological,
        } => {
            print_query_results(
                get_entries(priority, from, to, reversed, chronological, &pool).await?,
                extended,
            );
        }
        Commands::Delete { id } => delete_by_id(id, &pool).await?,
        Commands::Prune {} => prune(&pool).await?,
    }
    Ok(())
}

/// CLI Todo.
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Uses the global todo list instead of project-specific ones.
    #[arg(short, long, default_value_t = false)]
    global: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Add TODO entry.
    Add {
        /// Text describing the TODO task.
        text: String,

        /// Priority of the TODO task.
        #[arg(short, long)]
        priority: Priority,
    },

    /// Delete TODO entry based on its id.
    Delete { id: i64 },

    /// Queries TODO entries based on the parameters.
    Get {
        /// Filters by entries with the given priority.
        #[arg(short, long)]
        priority: Option<Priority>,

        /// Filters by entries that are more recent than the given datetime. Inclusive.
        #[arg(short, long, value_parser = to_datetime_from)]
        from: Option<DateTime<Local>>,

        /// Filters by entries that are older than the given datetime. Inclusive.
        #[arg(short, long, value_parser = to_datetime_to)]
        to: Option<DateTime<Local>>,

        /// Displays datetimes in extended mode, i.e. with hours, mins, secs and time zone.
        #[arg(short, long, default_value_t = false)]
        extended: bool,

        /// Reverses the order displayed on the query. The default is more recent entries on the top.
        #[arg(short, long, default_value_t = false)]
        reversed: bool,

        /// Sticks to chronological order sort only, disregarding priority.
        #[arg(short, long, default_value_t = false)]
        chronological: bool,
    },

    /// Prunes all entries, also resetting ids.
    Prune {},
}

/// Transforms string to datetime.
///
/// If string is in date format, then sets hours, mins and secs to 0.
fn to_datetime_from(s: &str) -> Result<DateTime<Local>, String> {
    if let Ok(x) = DateTime::from_str(s) {
        Ok(x)
    } else if let Ok(x) = NaiveDate::from_str(s) {
        let date_with_hms = x
            .and_hms_opt(0, 0, 0)
            .expect("All zeroes should be valid inputs.");
        Ok(date_with_hms.and_local_timezone(Local).unwrap())
    } else {
        Err("Invalid input for date/datetime.".to_string())
    }
}

/// Transforms string to datetime.
///
/// If string is in date format, then sets hours, min and secs to 11, 59 and 59 respectively.
fn to_datetime_to(s: &str) -> Result<DateTime<Local>, String> {
    if let Ok(x) = DateTime::from_str(s) {
        Ok(x)
    } else if let Ok(date_with_hms) = NaiveDate::from_str(s) {
        let oi = date_with_hms
            .and_hms_opt(11, 59, 59)
            .expect("11, 59, 59 should be valid inputs.");
        Ok(oi.and_local_timezone(Local).unwrap())
    } else {
        Err("Invalida input for date/datetime.".to_string())
    }
}

trait Extendable {
    fn get_style(&self, extended: bool) -> String;
}

impl Extendable for DateTime<Local> {
    /// Prints extended or non-extended mode.
    ///
    /// Extended mode consists of entire ISO timestamp, whereas non-extended
    /// consists of only the date (i.e., YYYY-MM-DD).
    fn get_style(&self, extended: bool) -> String {
        if extended {
            self.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
        } else {
            self.date_naive().to_string()
        }
    }
}

#[derive(Debug, ValueEnum, Clone)]
enum Priority {
    Normal = 0,
    Important = 1,
    Critical = 2,
}

impl Priority {
    fn from_i64(i: i64) -> Result<Self, ()> {
        match i {
            0 => Ok(Priority::Normal),
            1 => Ok(Priority::Important),
            2 => Ok(Priority::Critical),
            _ => Err(()),
        }
    }
}

impl ToString for Priority {
    fn to_string(&self) -> String {
        match self {
            Priority::Normal => "NORMAL".to_string(),
            Priority::Important => "IMPORTANT".to_string(),
            Priority::Critical => "CRITICAL".to_string(),
        }
    }
}

#[derive(Debug, FromRow)]
struct TodoEntry {
    id: i64,
    date: String,
    text: String,
    priority: i64,
}

#[derive(Debug, Clone)]
struct Todo {
    id: i64,
    date: DateTime<Local>,
    text: String,
    priority: Priority,
}

impl Todo {
    /// Transforms TodoEntry into Todo.
    fn from_entry(entry: &TodoEntry) -> Result<Self, ParseError> {
        Ok(Todo {
            id: entry.id,
            date: DateTime::from_str(&entry.date)?,
            text: entry.text.to_owned(),
            priority: Priority::from_i64(entry.priority).expect("Expected integer from 0 to 2."),
        })
    }
}

/// Posts new TODO into database.
async fn post_todo(text: &str, pool: &Pool<Sqlite>, priority: Priority) -> Result<(), sqlx::Error> {
    let now = time::SystemTime::now();
    let to_store = DateTime::<Local>::from(now).to_string();
    let priority = priority as i64;

    let oi = sqlx::query!(
        "INSERT INTO todos (date, text, priority) VALUES (?, ?, ?)",
        to_store,
        text,
        priority
    );

    oi.execute(pool).await?;

    Ok(())
}

/// Gets entries from TODO list according to parameters selected.
async fn get_entries(
    priority: Option<Priority>,
    from: Option<DateTime<Local>>,
    to: Option<DateTime<Local>>,
    reversed: bool,
    chronological: bool,
    pool: &Pool<Sqlite>,
) -> Result<Vec<Todo>, sqlx::Error> {
    let mut query = QueryBuilder::new("SELECT * from todos WHERE 1=1");

    if let Some(x) = priority {
        query.push(" AND priority = ");
        query.push_bind(x as i64);
    }

    if let Some(x) = from {
        query.push(" AND date >= ");
        query.push_bind(x.to_rfc3339());
    }

    if let Some(x) = to {
        query.push(" AND date <= ");
        query.push_bind(x.to_rfc3339());
    }

    if reversed {
        query.push(" ORDER BY date ASC");
    } else {
        query.push(" ORDER BY date DESC");
    }

    let query = query.build();

    let entries: Vec<TodoEntry> = query
        .fetch_all(pool)
        .await?
        .iter()
        .map(|x| TodoEntry::from_row(x).expect("Database entries should always be convertible."))
        .collect();

    let mut todos: Vec<Todo> = entries
        .iter()
        .map(|x| Todo::from_entry(x).expect("TodoEntries should always be convert to Todo."))
        .collect();

    if !chronological {
        todos = todos
            .iter()
            .filter(|x| matches!(x.priority, Priority::Critical))
            .chain(
                todos
                    .iter()
                    .filter(|x| matches!(x.priority, Priority::Important))
                    .chain(
                        todos
                            .iter()
                            .filter(|x| matches!(x.priority, Priority::Normal)),
                    ),
            )
            .cloned()
            .collect();
    }

    Ok(todos)
}

/// Deletes a database row via its id.
async fn delete_by_id(id: i64, pool: &Pool<Sqlite>) -> Result<(), sqlx::Error> {
    let q = query!("DELETE FROM todos WHERE id = ?", id);

    q.execute(pool).await?;

    Ok(())
}

/// Deletes all entries of database, also resetting the ids.
async fn prune(pool: &Pool<Sqlite>) -> Result<(), sqlx::Error> {
    let q = query!("DELETE FROM todos");

    q.execute(pool).await?;

    Ok(())
}

/// Prints results from queries with specific stylings.
fn print_query_results(results: Vec<Todo>, extended: bool) {
    if results.is_empty() {
        println!("No results found.");
        return;
    }

    let stdout = io::stdout();
    let mut handle = io::BufWriter::new(stdout.lock());

    for result in results {
        match result.priority {
            Priority::Critical => writeln!(
                handle,
                "{}{}: {:<9}: {}: {}",
                "#".red(),
                result.id.to_string().red(),
                result.priority.to_string().red(),
                result.date.get_style(extended).red(),
                result.text.red()
            )
            .expect("There should be no problems writing to stdout."),
            Priority::Important => writeln!(
                handle,
                "{}{}: {:<9}: {}: {}",
                "#".yellow(),
                result.id.to_string().yellow(),
                result.priority.to_string().yellow(),
                result.date.get_style(extended).to_string().yellow(),
                result.text.yellow()
            )
            .expect("There should be no problems writing to stdout."),
            Priority::Normal => writeln!(
                handle,
                "{}{}: {:<9}: {}: {}",
                "#",
                result.id.to_string(),
                result.priority.to_string(),
                result.date.get_style(extended),
                result.text
            )
            .expect("There should be no problems writing to stdout."),
        }
    }
}

/// Returns a pool of connections to the sqlite database.
async fn get_connection(global: bool) -> Result<Pool<Sqlite>, sqlx::Error> {
    let cltodo_folder = if global {
        home_dir()
            .expect("Home directory should be accessible.")
            .join(DB_FOLDER)
    } else if let Ok(output) = Command::new("git")
        .arg("rev-parse")
        .arg("--show-toplevel")
        .output()
    {
        PathBuf::from(
            std::str::from_utf8(&output.stdout)
                .expect("Should have utf8 output.")
                .trim(),
        )
        .join(DB_FOLDER)
    } else {
        home_dir()
            .expect("Home directory should be accessible.")
            .join(DB_FOLDER)
    };
    println!("{:?}", cltodo_folder);

    create_dir_all(&cltodo_folder).unwrap_or_else(|_| {
        panic!(
            "It should be possible to create the {} directory",
            DB_FOLDER
        )
    });

    let data_file = cltodo_folder.join(DB_FILE);
    let database_url = data_file
        .to_str()
        .expect("Data file path should be convertible to string.")
        .to_owned();

    let database_url = database_url.trim_start_matches("\\\\?\\");

    let creation = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(database_url);

    if creation.is_ok() {
        println!("Database file created at {}", database_url)
    }

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&format!("sqlite:///{}", database_url))
        .await
}
