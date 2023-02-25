use chrono::{DateTime, FixedOffset, Local, ParseError};
use dotenvy::dotenv;
use sqlx::{
    query_as,
    sqlite::{SqlitePoolOptions, SqliteQueryResult},
    Pool, Sqlite,
};
use std::{env, str::FromStr, time};

struct TodoEntry {
    id: i64,
    date: String,
    text: String,
}

#[derive(Debug)]
struct Todo {
    id: i64,
    date: DateTime<FixedOffset>,
    text: String,
}

impl Todo {
    fn from_entry(entry: &TodoEntry) -> Result<Self, ParseError> {
        Ok(Todo {
            id: entry.id,
            date: DateTime::from_str(&entry.date)?,
            text: entry.text.to_owned(),
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
        text TEXT NOT NULL
    ) STRICT"
    );

    query.execute(&pool).await?;

    post_todo("tenho que comprar uma tesoura", &pool).await?;

    let entry = query_as!(TodoEntry, "SELECT * FROM todos WHERE id = ?", 1)
        .fetch_one(&pool)
        .await?;

    let entry = Todo::from_entry(&entry);

    println!("{:?}", entry);

    Ok(())
}

async fn post_todo(text: &str, pool: &Pool<Sqlite>) -> Result<(), sqlx::Error> {
    let now = time::SystemTime::now();
    let to_store = DateTime::<Local>::from(now).to_string();

    let oi = sqlx::query!(
        "INSERT INTO todos (date, text) VALUES (?, ?)",
        to_store,
        text
    );

    oi.execute(pool).await?;

    Ok(())
}
