use chrono::{DateTime, FixedOffset, Local, ParseError};
use dotenvy::dotenv;
use sqlx::{sqlite::SqlitePoolOptions, FromRow, Pool, QueryBuilder, Sqlite};
use std::{path::PathBuf, str::FromStr, time};

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Add {
        #[arg(short, long)]
        text: String,

        #[arg(short, long)]
        priority: Priority,
    },
    Remove {
        #[arg(short, long)]
        id: i64,
    },

    /// does testing things
    Get {
        /// lists test values
        #[arg(short, long)]
        priority: Option<Priority>,
        from: Option<DateTime<FixedOffset>>,
        to: Option<DateTime<FixedOffset>>,
        reversed: Option<bool>,
    },
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

#[derive(Debug, FromRow)]
struct TodoEntry {
    id: i64,
    date: String,
    text: String,
    priority: i64,
}

#[derive(Debug)]
struct Todo {
    id: i64,
    date: DateTime<FixedOffset>,
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
    dotenv().expect(".env file not found");

    let database_url = dotenvy::var("DATABASE_URL").unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let query = sqlx::query!(
        "CREATE TABLE IF NOT EXISTS todos (
        id INTEGER PRIMARY KEY,
        date TEXT NOT NULL,
        text TEXT NOT NULL,
        priority INTEGER NOT NULL
    ) STRICT"
    );

    query.execute(&pool).await?;

    post_todo("eita", &pool, Priority::Important).await?;

    println!("{:?}", get_entries(None, None, None, false, &pool).await?);

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
    priority_level: Option<Priority>,
    from: Option<DateTime<FixedOffset>>,
    to: Option<DateTime<FixedOffset>>,
    reversed: bool,
    pool: &Pool<Sqlite>,
) -> Result<Vec<TodoEntry>, sqlx::Error> {
    let mut query = QueryBuilder::new("SELECT * from todos WHERE 1=1");

    if let Some(x) = priority_level {
        query.push("AND priority = ");
        query.push_bind(x as i64);
    }

    if let Some(x) = from {
        query.push("AND from >= ");
        query.push_bind(x.to_string());
    }

    if let Some(x) = to {
        query.push("AND from < ");
        query.push_bind(x.to_string());
    }

    if reversed {
        query.push(" ORDER BY date ASC");
    } else {
        query.push(" ORDER BY date DESC");
    }

    let query = query.build();
    let hey: Vec<TodoEntry> = query
        .fetch_all(pool)
        .await?
        .iter()
        .map(|x| TodoEntry::from_row(x).expect("Couldn't convert query result to TodoEntry"))
        .collect();

    Ok(hey)
}
