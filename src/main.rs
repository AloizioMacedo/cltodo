use chrono::{DateTime, Local, NaiveDate, ParseError};
use home::home_dir;
use sqlx::{query, sqlite::SqlitePoolOptions, FromRow, Pool, QueryBuilder, Sqlite};
use std::{
    fs::{create_dir_all, read_dir, OpenOptions},
    str::FromStr,
    time,
};

use clap::{Parser, Subcommand, ValueEnum};
use colored::Colorize;

const DB_FOLDER: &str = ".cltodo";
const DB_FILE: &str = "data.db";

/// CLI Todo.
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
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

fn to_datetime_from(s: &str) -> Result<DateTime<Local>, String> {
    if let Ok(x) = DateTime::from_str(s) {
        return Ok(x);
    } else {
        if let Ok(x) = NaiveDate::from_str(s) {
            let oi = x.and_hms_opt(0, 0, 0).unwrap();
            Ok(oi.and_local_timezone(Local).unwrap())
        } else {
            Err("oh-oh".to_string())
        }
    }
}

fn to_datetime_to(s: &str) -> Result<DateTime<Local>, String> {
    if let Ok(x) = DateTime::from_str(s) {
        return Ok(x);
    } else {
        if let Ok(x) = NaiveDate::from_str(s) {
            let oi = x.and_hms_opt(11, 59, 59).unwrap();
            Ok(oi.and_local_timezone(Local).unwrap())
        } else {
            Err("oh-oh".to_string())
        }
    }
}

trait Extendable {
    fn get_style(&self, extended: bool) -> String;
}

impl Extendable for DateTime<Local> {
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
    fn from_entry(entry: &TodoEntry) -> Result<Self, ParseError> {
        Ok(Todo {
            id: entry.id,
            date: DateTime::from_str(&entry.date)?,
            text: entry.text.to_owned(),
            priority: Priority::from_i64(entry.priority).expect("Expected integer from 0 to 2."),
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let args = Cli::parse();

    let pool = get_connection().await?;

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
        .map(|x| TodoEntry::from_row(x).expect("Couldn't convert query result to TodoEntry"))
        .collect();

    let mut todos: Vec<Todo> = entries
        .iter()
        .map(|x| Todo::from_entry(x).unwrap())
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

async fn delete_by_id(id: i64, pool: &Pool<Sqlite>) -> Result<(), sqlx::Error> {
    let q = query!("DELETE FROM todos WHERE id = ?", id);

    q.execute(pool).await?;

    Ok(())
}

async fn prune(pool: &Pool<Sqlite>) -> Result<(), sqlx::Error> {
    let q = query!("DELETE FROM todos");

    q.execute(pool).await?;

    Ok(())
}

fn print_query_results(results: Vec<Todo>, extended: bool) {
    if results.is_empty() {
        println!("No results found.");
        return;
    }

    for result in results {
        match result.priority {
            Priority::Critical => println!(
                "{}{}: {:<9}: {}: {}",
                "#".red(),
                result.id.to_string().red(),
                result.priority.to_string().red(),
                result.date.get_style(extended).red(),
                result.text.red()
            ),
            Priority::Important => println!(
                "{}{}: {:<9}: {}: {}",
                "#".yellow(),
                result.id.to_string().yellow(),
                result.priority.to_string().yellow(),
                result.date.get_style(extended).to_string().yellow(),
                result.text.yellow()
            ),
            Priority::Normal => println!(
                "{}{}: {:<9}: {}: {}",
                "#",
                result.id.to_string(),
                result.priority.to_string(),
                result.date.get_style(extended),
                result.text
            ),
        }
    }
}

async fn get_connection() -> Result<Pool<Sqlite>, sqlx::Error> {
    let current_dir = std::env::current_dir().unwrap();

    let mut this_dir = read_dir(&current_dir).unwrap();
    let has_git = this_dir.any(|x| x.as_ref().unwrap().file_name() == ".git");

    let cltodo_folder = if has_git {
        current_dir.join(DB_FOLDER)
    } else {
        home_dir().unwrap().join(DB_FOLDER)
    };

    create_dir_all(&cltodo_folder).unwrap();

    let data_file = cltodo_folder.join(DB_FILE);
    let database_url = data_file.to_str().unwrap().to_owned();

    let database_url = database_url.trim_start_matches("\\\\?\\");

    let creation = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(database_url);

    match creation {
        Ok(_) => println!("Database file created at {}", database_url),
        _ => (),
    }

    Ok(SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&format!("sqlite:///{}", database_url))
        .await?)
}
