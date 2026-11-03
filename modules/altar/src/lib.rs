pub mod data;
pub mod detect;
pub mod evaluate;
pub mod fill;
pub mod matching;
pub mod read;
pub mod text;
pub mod verdict;

pub use data::{AltarKind, Card, Side, Target, load_cards, repo_data_dir};
pub use matching::{Match, match_box};
