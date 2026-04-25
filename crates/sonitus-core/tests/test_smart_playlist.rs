//! Integration test: smart playlist rules engine evaluates against a
//! seeded library and returns the expected tracks.

use sonitus_core::crypto::VaultDb;
use sonitus_core::library::{Track, queries};
use sonitus_core::playlist::smart::{
    Combinator, SmartCondition, SmartField, SmartOp, SmartRules, SortOrder, evaluate,
};
use serde_json::json;

fn fresh_track(id: &str, source: &str, title: &str, genre: Option<&str>, year: Option<i32>, plays: i64) -> Track {
    Track {
        id: id.into(),
        title: title.into(),
        artist_id: None,
        album_artist_id: None,
        album_id: None,
        source_id: source.into(),
        remote_path: format!("/{id}.mp3"),
        local_cache_path: None,
        duration_ms: Some(180_000),
        track_number: None,
        disc_number: 1,
        genre: genre.map(str::to_string),
        year,
        bpm: None,
        replay_gain_track: None,
        replay_gain_album: None,
        file_size_bytes: None,
        format: Some("mp3".into()),
        bitrate_kbps: None,
        sample_rate_hz: None,
        bit_depth: None,
        channels: None,
        content_hash: None,
        musicbrainz_id: None,
        play_count: plays,
        last_played_at: None,
        rating: None,
        loved: 0,
        created_at: 0,
        updated_at: 0,
    }
}

async fn seed_library() -> VaultDb {
    let db = VaultDb::open_in_memory().await.unwrap();
    sqlx::query("INSERT INTO sources (id, name, kind, config_json) VALUES ('s', 'T', 'local', '{}')")
        .execute(db.pool()).await.unwrap();
    let rows = vec![
        fresh_track("a", "s", "Rock1",  Some("Rock"),       Some(2003), 5),
        fresh_track("b", "s", "Rock2",  Some("Rock"),       Some(2010), 100),
        fresh_track("c", "s", "Jazz1",  Some("Jazz"),       Some(1970), 0),
        fresh_track("d", "s", "Elec1",  Some("Electronic"), Some(2022), 50),
        fresh_track("e", "s", "Untag",  None,               None,       0),
    ];
    for t in &rows {
        queries::tracks::upsert(db.pool(), t).await.unwrap();
    }
    db
}

#[tokio::test]
async fn smart_filter_by_genre_returns_matching_tracks() {
    let db = seed_library().await;
    let rules = SmartRules {
        conditions: vec![SmartCondition {
            field: SmartField::Genre,
            op: SmartOp::Eq,
            value: json!("Rock"),
        }],
        combinator: Combinator::And,
        sort: SortOrder::Default,
        limit: None,
    };
    let matches = evaluate(db.pool(), &rules).await.unwrap();
    let titles: std::collections::HashSet<_> = matches.iter().map(|t| t.title.clone()).collect();
    assert_eq!(titles, ["Rock1".to_string(), "Rock2".to_string()].into_iter().collect());
}

#[tokio::test]
async fn smart_filter_combines_year_range() {
    let db = seed_library().await;
    let rules = SmartRules {
        conditions: vec![
            SmartCondition { field: SmartField::Year, op: SmartOp::Gte, value: json!(2000) },
            SmartCondition { field: SmartField::Year, op: SmartOp::Lt,  value: json!(2015) },
        ],
        combinator: Combinator::And,
        sort: SortOrder::Default,
        limit: None,
    };
    let matches = evaluate(db.pool(), &rules).await.unwrap();
    let titles: std::collections::HashSet<_> = matches.iter().map(|t| t.title.clone()).collect();
    assert_eq!(titles, ["Rock1".to_string(), "Rock2".to_string()].into_iter().collect());
}

#[tokio::test]
async fn smart_filter_or_combinator_unions_results() {
    let db = seed_library().await;
    let rules = SmartRules {
        conditions: vec![
            SmartCondition { field: SmartField::Genre, op: SmartOp::Eq, value: json!("Jazz") },
            SmartCondition { field: SmartField::Genre, op: SmartOp::Eq, value: json!("Electronic") },
        ],
        combinator: Combinator::Or,
        sort: SortOrder::Default,
        limit: None,
    };
    let matches = evaluate(db.pool(), &rules).await.unwrap();
    let titles: std::collections::HashSet<_> = matches.iter().map(|t| t.title.clone()).collect();
    assert_eq!(titles, ["Jazz1".to_string(), "Elec1".to_string()].into_iter().collect());
}

#[tokio::test]
async fn smart_filter_most_played_sorts_descending() {
    let db = seed_library().await;
    let rules = SmartRules {
        conditions: vec![SmartCondition {
            field: SmartField::PlayCount,
            op: SmartOp::Gt,
            value: json!(0),
        }],
        combinator: Combinator::And,
        sort: SortOrder::MostPlayed,
        limit: Some(2),
    };
    let matches = evaluate(db.pool(), &rules).await.unwrap();
    assert_eq!(matches.len(), 2);
    // Top two by play_count: b (100), d (50).
    assert_eq!(matches[0].title, "Rock2");
    assert_eq!(matches[1].title, "Elec1");
}

#[tokio::test]
async fn smart_filter_contains_uses_like() {
    let db = seed_library().await;
    let rules = SmartRules {
        conditions: vec![SmartCondition {
            field: SmartField::Genre,
            op: SmartOp::Contains,
            value: json!("ock"),
        }],
        combinator: Combinator::And,
        sort: SortOrder::Default,
        limit: None,
    };
    let matches = evaluate(db.pool(), &rules).await.unwrap();
    let titles: std::collections::HashSet<_> = matches.iter().map(|t| t.title.clone()).collect();
    // "Rock" contains "ock"; nothing else does.
    assert_eq!(titles, ["Rock1".to_string(), "Rock2".to_string()].into_iter().collect());
}
