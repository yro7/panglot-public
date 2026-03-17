use serde::{Deserialize, Serialize};

// ----- Stored Card -----

/// Represents a card fetched from the storage backend (Anki or Local DB).
/// Contains the data needed to reconstruct the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCard {
    /// The unique ID of the card/note in the storage system.
    pub note_id: String,
    /// The unique ID of the specific review card (if applicable).
    pub card_id: String,
    /// The raw fields content (can be separated by \x1f or JSON depending on backend).
    pub fields: String,
    /// The card's tags (space-separated string).
    pub tags: String,
    /// The card's interval in days (0.0 = new, >= 21.0 = mature).
    pub interval_days: f64,
    /// Number of lapses (times the card was forgotten).
    pub lapses: i32,
}

impl StoredCard {
    /// Returns true if the card is considered "mature" (e.g., interval >= 21 days).
    pub fn is_mature(&self) -> bool {
        self.interval_days >= 21.0
    }

    /// Returns true if the card has the "leech" tag.
    pub fn is_leech(&self) -> bool {
        self.tags.split_whitespace().any(|t| t == "leech")
    }
}

// ----- Deck Info -----

/// Summary information about a deck fetched from the storage backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckInfo {
    /// The unique ID of the deck.
    pub deck_id: String,
    /// The name of the deck.
    pub name: String,
    /// Total number of cards in the deck.
    pub card_count: usize,
    /// Number of new cards (Nouvelles).
    pub new_count: usize,
    /// Number of learning cards (En cours).
    pub learning_count: usize,
    /// Number of review cards (À réviser).
    pub review_count: usize,
    /// Indicates whether the deck contains explicitly LC-generated cards.
    pub is_lc: bool,
}

// ----- Data representations for saving -----

/// A built card ready to be saved into the storage backend.
#[derive(Debug, Clone)]
pub struct NewCardEntry {
    pub front_html: String,
    pub back_html: String,
    pub skill_name: String,
    pub template_name: String,
    pub fields_json: String,
    pub explanation: String,
    pub ipa: String,
    pub metadata_json: String,
    pub audio_path: Option<String>,
}

/// A deck ready to be saved into the storage backend.
#[derive(Debug, Clone)]
pub struct NewDeckData {
    pub name: String,
    pub language_code: String,
    pub cards: Vec<NewCardEntry>,
}

// ----- StorageProvider Trait -----

/// Unified interface for interacting with card storage (Anki, Local DB, etc.).
#[async_trait::async_trait]
pub trait StorageProvider: Send + Sync {
    /// Fetches cards from the storage to be analyzed (e.g. for Lexicon extraction).
    async fn fetch_cards(&self) -> Result<Vec<StoredCard>, Box<dyn std::error::Error + Send + Sync>>;
    
    /// Fetches summary info for all available decks.
    async fn fetch_decks(&self) -> Result<Vec<DeckInfo>, Box<dyn std::error::Error + Send + Sync>>;
    
    /// Pushes a newly generated deck to the storage.
    async fn save_deck(&self, deck: &NewDeckData) -> Result<usize, Box<dyn std::error::Error + Send + Sync>>;

    /// Deletes a deck and all its associated cards and data.
    async fn delete_deck(&self, deck_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// A read-only provider that serves pre-fetched cards. Used to feed merged
/// card lists (e.g. Anki + local DB) into the lexicon scanner.
pub struct SnapshotProvider {
    cards: Vec<StoredCard>,
}

impl SnapshotProvider {
    pub fn new(cards: Vec<StoredCard>) -> Self {
        Self { cards }
    }
}

#[async_trait::async_trait]
impl StorageProvider for SnapshotProvider {
    async fn fetch_cards(&self) -> Result<Vec<StoredCard>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.cards.clone())
    }

    async fn fetch_decks(&self) -> Result<Vec<DeckInfo>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![])
    }

    async fn save_deck(&self, _deck: &NewDeckData) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        Err("SnapshotProvider is read-only".into())
    }

    async fn delete_deck(&self, _deck_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("SnapshotProvider is read-only".into())
    }
}
