//! The save files, and the fact that two processes share them.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a test that trips is a test that failed"
)]

use jiff::Timestamp;
use rust_decimal::Decimal;
use safe_invest_core::factory::{self, NewGame};
use safe_invest_core::model::{GameSession, PlayerKind};
use safe_invest_core::store::{GameStore, StoreError};
use safe_invest_core::{GameStore as _Alias, Paths};
use std::str::FromStr;

fn now() -> Timestamp {
    "2026-01-01T12:00:00Z".parse().unwrap()
}

fn store() -> (tempfile::TempDir, GameStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = GameStore::new(Paths::at(dir.path())).unwrap();
    (dir, store)
}

fn a_game(name: &str) -> GameSession {
    factory::create(
        NewGame {
            player_name: name.into(),
            player_kind: PlayerKind::Human,
            currency: "EUR".into(),
            starting_cash: Decimal::from(1000),
            fee_percent: Decimal::ZERO,
            goal: None,
        },
        now(),
    )
    .unwrap()
}

#[test]
fn a_saved_game_comes_back_byte_for_byte() {
    let (_dir, store) = store();
    let session = a_game("Alice");
    store.save(&session).unwrap();

    assert_eq!(store.load(session.id).unwrap(), session);
}

#[test]
fn loading_a_game_that_does_not_exist_says_so() {
    let (_dir, store) = store();
    let error = store.load(uuid::Uuid::new_v4()).unwrap_err();
    assert!(matches!(error, StoreError::NotFound(_)));
}

#[test]
fn the_game_list_is_ordered_by_most_recent_activity() {
    let (_dir, store) = store();

    let mut old = a_game("Ancien");
    old.updated_at = Timestamp::from_str("2025-01-01T00:00:00Z").unwrap();
    let mut recent = a_game("Récent");
    recent.updated_at = Timestamp::from_str("2026-06-01T00:00:00Z").unwrap();

    store.save(&old).unwrap();
    store.save(&recent).unwrap();

    let listed = store.list();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].player_name, "Récent");
}

#[test]
fn one_corrupt_file_does_not_hide_the_others() {
    let (dir, store) = store();
    let good = a_game("Intacte");
    store.save(&good).unwrap();
    std::fs::write(dir.path().join("games").join("broken.json"), b"{ not json").unwrap();

    let listed = store.list();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].player_name, "Intacte");
}

#[test]
fn the_current_game_pointer_survives_a_round_trip_and_can_be_cleared() {
    let (_dir, store) = store();
    let session = a_game("Alice");
    store.save(&session).unwrap();

    store.set_current_game(Some(session.id)).unwrap();
    assert_eq!(store.current_game(), Some(session.id));

    store.set_current_game(None).unwrap();
    assert_eq!(store.current_game(), None);
}

#[test]
fn deleting_a_game_also_forgets_it_as_the_current_one() {
    let (_dir, store) = store();
    let session = a_game("Alice");
    store.save(&session).unwrap();
    store.set_current_game(Some(session.id)).unwrap();

    store.delete(session.id).unwrap();

    assert!(matches!(
        store.load(session.id),
        Err(StoreError::NotFound(_))
    ));
    assert_eq!(store.current_game(), None);
}

#[test]
fn a_save_never_leaves_a_temporary_file_behind() {
    let (dir, store) = store();
    store.save(&a_game("Alice")).unwrap();

    let leftovers: Vec<_> = std::fs::read_dir(dir.path().join("games"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
        .collect();
    assert!(
        leftovers.is_empty(),
        "fichiers temporaires oubliés : {leftovers:?}"
    );
}

#[test]
fn concurrent_mutations_never_lose_a_single_one() {
    const WRITERS: usize = 8;
    const PER_WRITER: usize = 25;

    // The real scenario: the window and the MCP server both writing. Without
    // the lock held across read-modify-write, some of these increments vanish.
    let (_dir, store) = store();
    let session = a_game("Alice");
    store.save(&session).unwrap();
    let id = session.id;

    std::thread::scope(|scope| {
        for _ in 0..WRITERS {
            let store = store.clone();
            scope.spawn(move || {
                for _ in 0..PER_WRITER {
                    store
                        .mutate(id, |game: &mut GameSession| {
                            game.cash += Decimal::ONE;
                            Ok::<(), StoreError>(())
                        })
                        .unwrap();
                }
            });
        }
    });

    let final_cash = store.load(id).unwrap().cash;
    assert_eq!(
        final_cash,
        Decimal::from(1000 + WRITERS * PER_WRITER),
        "des écritures concurrentes ont été perdues"
    );
}

#[test]
fn a_failed_mutation_leaves_the_game_untouched() {
    let (_dir, store) = store();
    let session = a_game("Alice");
    store.save(&session).unwrap();

    let outcome: Result<(), StoreError> = store.mutate(session.id, |game| {
        game.cash = Decimal::from(999_999);
        Err(StoreError::NotFound(game.id))
    });

    assert!(outcome.is_err());
    assert_eq!(store.load(session.id).unwrap().cash, Decimal::from(1000));
}

#[test]
fn paths_follow_the_environment_override() {
    let paths = Paths::at("/tmp/safeinvest-test-root");
    assert!(paths.games_dir().ends_with("games"));
    assert!(paths.settings_file().ends_with("settings.json"));
    assert!(paths.lock_file().ends_with(".store.lock"));
}

// Keeps the re-export in `lib.rs` honest.
const _: fn() = || {
    let _: Option<_Alias> = None;
};
