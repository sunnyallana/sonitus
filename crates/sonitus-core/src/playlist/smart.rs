//! Smart playlist rules engine.
//!
//! Rules are an AST stored as JSON in the `playlists.smart_rules` column.
//! The engine compiles the AST to a SQL `WHERE` clause + parameters, then
//! runs it against the `tracks` table to produce the live track list.
//!
//! Supported fields: `genre`, `year`, `bpm`, `rating`, `loved`,
//! `play_count`, `last_played_at`, `created_at`, `duration_ms`,
//! `artist_name`, `album_title`, `format`.
//!
//! Operators: `eq`, `ne`, `lt`, `lte`, `gt`, `gte`, `contains`, `starts_with`.
//! Logical: `and` (default), `or`.

use crate::error::Result;
use crate::library::Track;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Top-level smart-playlist definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartRules {
    /// All conditions to combine.
    pub conditions: Vec<SmartCondition>,
    /// Whether to AND or OR the conditions.
    #[serde(default)]
    pub combinator: Combinator,
    /// Optional sort order.
    #[serde(default)]
    pub sort: SortOrder,
    /// Optional cap on number of tracks.
    pub limit: Option<i64>,
}

/// Logical combinator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Combinator {
    #[default]
    /// All conditions must match.
    And,
    /// Any condition may match.
    Or,
}

/// One condition in a smart playlist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartCondition {
    /// Field to compare.
    pub field: SmartField,
    /// Comparison operator.
    pub op: SmartOp,
    /// Value to compare against. Strings, numbers, and booleans are accepted.
    pub value: serde_json::Value,
}

/// Tracks fields the smart engine knows about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartField {
    /// Genre tag.
    Genre,
    /// Release year.
    Year,
    /// Beats per minute.
    Bpm,
    /// User-set rating (0-5).
    Rating,
    /// "Loved" flag.
    Loved,
    /// Times played by the user.
    PlayCount,
    /// Unix timestamp of last play.
    LastPlayedAt,
    /// Unix timestamp of insertion into the library.
    CreatedAt,
    /// Track duration in milliseconds.
    DurationMs,
    /// Joined artist name (artists.name).
    ArtistName,
    /// Joined album title (albums.title).
    AlbumTitle,
    /// Container/codec format.
    Format,
}

impl SmartField {
    /// SQL column or join expression for this field.
    pub fn sql(self) -> &'static str {
        match self {
            Self::Genre => "tracks.genre",
            Self::Year => "tracks.year",
            Self::Bpm => "tracks.bpm",
            Self::Rating => "tracks.rating",
            Self::Loved => "tracks.loved",
            Self::PlayCount => "tracks.play_count",
            Self::LastPlayedAt => "tracks.last_played_at",
            Self::CreatedAt => "tracks.created_at",
            Self::DurationMs => "tracks.duration_ms",
            Self::ArtistName => "(SELECT name FROM artists WHERE id = tracks.artist_id)",
            Self::AlbumTitle => "(SELECT title FROM albums WHERE id = tracks.album_id)",
            Self::Format => "tracks.format",
        }
    }
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartOp {
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
    /// Less than.
    Lt,
    /// Less than or equal.
    Lte,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Gte,
    /// Substring match (translates to SQL `LIKE %value%`).
    Contains,
    /// Prefix match (translates to SQL `LIKE value%`).
    StartsWith,
}

impl SmartOp {
    /// SQL comparison operator (or LIKE for fuzzy ops).
    pub fn sql(self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Lte => "<=",
            Self::Gt => ">",
            Self::Gte => ">=",
            Self::Contains | Self::StartsWith => "LIKE",
        }
    }
}

/// Sort order for the resulting tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    #[default]
    /// Insertion order (no ORDER BY).
    Default,
    /// Most-recently added first.
    RecentlyAdded,
    /// Most-recently played first.
    RecentlyPlayed,
    /// Most-played first.
    MostPlayed,
    /// Random (uses SQLite `RANDOM()`).
    Random,
}

impl SortOrder {
    /// SQL ORDER BY fragment.
    pub fn sql(self) -> &'static str {
        match self {
            Self::Default => "",
            Self::RecentlyAdded => " ORDER BY created_at DESC",
            Self::RecentlyPlayed => " ORDER BY last_played_at DESC",
            Self::MostPlayed => " ORDER BY play_count DESC",
            Self::Random => " ORDER BY RANDOM()",
        }
    }
}

/// Compile rules to a SQL query and parameter list, then execute.
pub async fn evaluate(pool: &SqlitePool, rules: &SmartRules) -> Result<Vec<Track>> {
    let mut sql = String::from("SELECT tracks.* FROM tracks WHERE 1");
    let glue = match rules.combinator {
        Combinator::And => " AND ",
        Combinator::Or => " OR ",
    };

    let mut binds: Vec<BindValue> = Vec::new();
    if !rules.conditions.is_empty() {
        sql.push_str(" AND (");
        for (i, c) in rules.conditions.iter().enumerate() {
            if i > 0 { sql.push_str(glue); }
            let lhs = c.field.sql();
            let op = c.op.sql();
            sql.push_str(&format!("{lhs} {op} ?"));
            binds.push(value_to_bind(&c.value, c.op));
        }
        sql.push(')');
    }

    sql.push_str(rules.sort.sql());
    if let Some(limit) = rules.limit {
        sql.push_str(&format!(" LIMIT {limit}"));
    }

    let mut q = sqlx::query_as::<_, Track>(&sql);
    for b in binds {
        q = match b {
            BindValue::Str(s) => q.bind(s),
            BindValue::Int(i) => q.bind(i),
            BindValue::Real(r) => q.bind(r),
        };
    }
    Ok(q.fetch_all(pool).await?)
}

enum BindValue {
    Str(String),
    Int(i64),
    Real(f64),
}

fn value_to_bind(v: &serde_json::Value, op: SmartOp) -> BindValue {
    match v {
        serde_json::Value::String(s) => match op {
            SmartOp::Contains => BindValue::Str(format!("%{s}%")),
            SmartOp::StartsWith => BindValue::Str(format!("{s}%")),
            _ => BindValue::Str(s.clone()),
        },
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { BindValue::Int(i) }
            else if let Some(f) = n.as_f64() { BindValue::Real(f) }
            else { BindValue::Int(0) }
        }
        serde_json::Value::Bool(b) => BindValue::Int(if *b { 1 } else { 0 }),
        _ => BindValue::Str(v.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_round_trip_through_json() {
        let rules = SmartRules {
            conditions: vec![SmartCondition {
                field: SmartField::Genre,
                op: SmartOp::Eq,
                value: serde_json::Value::String("Rock".into()),
            }],
            combinator: Combinator::And,
            sort: SortOrder::MostPlayed,
            limit: Some(50),
        };
        let json = serde_json::to_string(&rules).unwrap();
        let back: SmartRules = serde_json::from_str(&json).unwrap();
        assert_eq!(back.conditions.len(), 1);
        assert_eq!(back.combinator, Combinator::And);
        assert_eq!(back.sort, SortOrder::MostPlayed);
        assert_eq!(back.limit, Some(50));
    }

    #[test]
    fn smart_op_contains_uses_like() {
        assert_eq!(SmartOp::Contains.sql(), "LIKE");
    }

    #[test]
    fn sort_order_emits_correct_sql() {
        assert!(SortOrder::MostPlayed.sql().contains("play_count"));
        assert!(SortOrder::RecentlyAdded.sql().contains("created_at"));
        assert_eq!(SortOrder::Default.sql(), "");
    }
}
