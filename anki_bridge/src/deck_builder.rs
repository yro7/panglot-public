use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use lc_core::storage::{NewDeckData};

// ----- Shared model definition (used by export_apkg) -----

const MODEL_CSS: &str = "\
.card { font-family: 'Segoe UI', sans-serif; font-size: 18px; text-align: center; color: #e0e0e0; background-color: #2b2b2b; padding: 20px; }\
.translation { color: #b39ddb; font-size: 16px; margin-bottom: 8px; }\
.cloze-sentence { color: #e0e0e0; font-size: 22px; font-weight: bold; margin: 12px 0; }\
.full-sentence { color: #e0e0e0; font-size: 22px; margin: 12px 0; }\
.hint { color: #ef5350; font-size: 14px; margin-top: 4px; }\
.skill-name { color: #66bb6a; font-weight: bold; font-size: 16px; margin: 8px 0; }\
.ipa { font-family: 'Gentium Plus', 'Charis SIL', serif; color: #bdbdbd; font-size: 16px; margin: 8px 0; }\
.text-prompt { color: #90caf9; font-size: 22px; font-weight: bold; }\
.transcript { color: #e0e0e0; font-size: 18px; margin: 8px 0; }\
.listen-prompt { color: #ffab91; font-size: 20px; font-weight: bold; }\
.explanation { text-align: left; font-size: 14px; margin-top: 12px; padding: 10px; background: #333; border-radius: 6px; }\
.audio { margin: 10px 0; }";

const MODEL_QFMT: &str = "{{Front}}";

const MODEL_AFMT: &str = "{{FrontSide}}\
<hr id=answer>\
{{#SkillName}}<div class=\"skill-name\">{{SkillName}}</div>{{/SkillName}}\
{{#IPA}}<div class=\"ipa\">{{IPA}}</div>{{/IPA}}\
{{Back}}\
<div class=\"audio\">{{Audio}}</div>\
{{#Explanation}}<div class=\"explanation\">{{Explanation}}</div>{{/Explanation}}\
<div style=\"display:none\">{{Metadata}}</div>";

// ----- DeckBuilder -----

/// Assembles generated cards into an Anki-compatible deck.
///
/// The DeckBuilder:
/// 1. Takes thoroughly constructed `NewDeckData`.
/// 2. Exports to `.apkg` format (SQLite DB inside a ZIP archive)
pub struct DeckBuilder {
    pub deck_data: NewDeckData,
}

impl DeckBuilder {
    pub fn new(deck_data: NewDeckData) -> Self {
        Self { deck_data }
    }

    /// Returns the number of cards currently in the deck.
    pub fn card_count(&self) -> usize {
        self.deck_data.cards.len()
    }

    /// Exports the deck as an `.apkg` file.
    ///
    /// An `.apkg` file is a ZIP archive containing:
    /// - `collection.anki2`: A SQLite database with notes, cards, and deck configuration
    /// - `media`: A JSON file mapping media filenames to their indices
    pub fn export_apkg(&self, path: impl AsRef<Path>) -> Result<(), Box<dyn std::error::Error>> {
        let path = path.as_ref();

        // Create the SQLite database in memory
        let conn = rusqlite::Connection::open_in_memory()?;
        self.create_anki_schema(&conn)?;
        self.insert_deck_config(&conn)?;
        self.insert_note_type(&conn)?;
        self.insert_cards(&conn)?;

        // Write the database to a temporary file
        let db_bytes = Self::serialize_db(&conn)?;

        // Package into a ZIP file
        let file = fs::File::create(path)?;
        let mut zip = zip::ZipWriter::new(file);

        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        // Add collection.anki2
        zip.start_file("collection.anki2", options)?;
        zip.write_all(&db_bytes)?;

        // Package audio media files into the ZIP
        let mut media_map: HashMap<String, String> = HashMap::new();
        let mut media_index: usize = 0;

        for (card_idx, entry) in self.deck_data.cards.iter().enumerate() {
            if let Some(audio_path) = &entry.audio_path {
                let file_path = Path::new(audio_path);
                tracing::debug!(card_idx, audio_path, "Checking audio file");
                if file_path.exists() {
                    let filename = file_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("audio.mp3")
                        .to_string();

                    let audio_bytes = fs::read(file_path)?;
                    tracing::debug!(card_idx, filename, bytes = audio_bytes.len(), media_index, "Packing audio into apkg");

                    let index_str = media_index.to_string();
                    zip.start_file(&index_str, options)?;
                    zip.write_all(&audio_bytes)?;

                    media_map.insert(index_str, filename);
                    media_index += 1;
                } else {
                    tracing::warn!(audio_path, "Audio file not found, skipping");
                }
            }
        }

        // Write the media mapping JSON
        let media_json = serde_json::to_string(&media_map)?;
        zip.start_file("media", options)?;
        zip.write_all(media_json.as_bytes())?;

        zip.finish()?;
        Ok(())
    }

    fn create_anki_schema(
        &self,
        conn: &rusqlite::Connection,
    ) -> Result<(), Box<dyn std::error::Error>> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS col (
                id INTEGER PRIMARY KEY,
                crt INTEGER NOT NULL,
                mod INTEGER NOT NULL,
                scm INTEGER NOT NULL,
                ver INTEGER NOT NULL,
                dty INTEGER NOT NULL,
                usn INTEGER NOT NULL,
                ls INTEGER NOT NULL,
                conf TEXT NOT NULL,
                models TEXT NOT NULL,
                decks TEXT NOT NULL,
                dconf TEXT NOT NULL,
                tags TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS notes (
                id INTEGER PRIMARY KEY,
                guid TEXT NOT NULL,
                mid INTEGER NOT NULL,
                mod INTEGER NOT NULL,
                usn INTEGER NOT NULL,
                tags TEXT NOT NULL,
                flds TEXT NOT NULL,
                sfld TEXT NOT NULL,
                csum INTEGER NOT NULL,
                flags INTEGER NOT NULL,
                data TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS cards (
                id INTEGER PRIMARY KEY,
                nid INTEGER NOT NULL,
                did INTEGER NOT NULL,
                ord INTEGER NOT NULL,
                mod INTEGER NOT NULL,
                usn INTEGER NOT NULL,
                type INTEGER NOT NULL,
                queue INTEGER NOT NULL,
                due INTEGER NOT NULL,
                ivl INTEGER NOT NULL,
                factor INTEGER NOT NULL,
                reps INTEGER NOT NULL,
                lapses INTEGER NOT NULL,
                left INTEGER NOT NULL,
                odue INTEGER NOT NULL,
                odid INTEGER NOT NULL,
                flags INTEGER NOT NULL,
                data TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS revlog (
                id INTEGER PRIMARY KEY,
                cid INTEGER NOT NULL,
                usn INTEGER NOT NULL,
                ease INTEGER NOT NULL,
                ivl INTEGER NOT NULL,
                lastIvl INTEGER NOT NULL,
                factor INTEGER NOT NULL,
                time INTEGER NOT NULL,
                type INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS graves (
                usn INTEGER NOT NULL,
                oid INTEGER NOT NULL,
                type INTEGER NOT NULL
            );"
        )?;
        Ok(())
    }

    fn insert_deck_config(
        &self,
        conn: &rusqlite::Connection,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;

        // Generate a stable deck_id from the deck name so it doesn't collide with
        // Anki's built-in default deck (id=1) and stays consistent across re-exports.
        let deck_id: i64 = {
            use std::hash::{DefaultHasher, Hash, Hasher};
            let mut h = DefaultHasher::new();
            self.deck_data.name.hash(&mut h);
            // Keep in the positive i64 range, avoid 0 and 1 (Anki reserved)
            (h.finish() % 1_000_000_000_000 + 2) as i64
        };
        let model_id: i64 = 1_400_000_000_000;

        let decks = serde_json::json!({
            deck_id.to_string(): {
                "id": deck_id,
                "name": self.deck_data.name,
                "mod": now,
                "usn": -1,
                "lrnToday": [0, 0],
                "revToday": [0, 0],
                "newToday": [0, 0],
                "timeToday": [0, 0],
                "collapsed": false,
                "desc": "",
                "dyn": 0,
                "conf": 1,
                "extendRev": 0,
                "extendNew": 0
            }
        });

        let models = serde_json::json!({
            model_id.to_string(): {
                "id": model_id,
                "name": "Panglot",
                "mod": now,
                "type": 0,
                "sortf": 0,
                "vers": [],
                "latexPre": "\\documentclass[12pt]{article}\n\\special{papersize=3in,5in}\n\\usepackage{amssymb,amsmath}\n\\pagestyle{empty}\n\\setlength{\\parindent}{0in}\n\\begin{document}\n",
                "latexPost": "\\end{document}",
                "latexsvg": false,
                "css": MODEL_CSS,
                "flds": [
                    {"name": "Front", "ord": 0, "sticky": false, "rtl": false, "font": "Arial", "size": 20, "media": [], "id": 1},
                    {"name": "Back", "ord": 1, "sticky": false, "rtl": false, "font": "Arial", "size": 20, "media": [], "id": 2},
                    {"name": "SkillName", "ord": 2, "sticky": false, "rtl": false, "font": "Arial", "size": 20, "media": [], "id": 3},
                    {"name": "IPA", "ord": 3, "sticky": false, "rtl": false, "font": "Arial", "size": 20, "media": [], "id": 4},
                    {"name": "Audio", "ord": 4, "sticky": false, "rtl": false, "font": "Arial", "size": 20, "media": [], "id": 5},
                    {"name": "Explanation", "ord": 5, "sticky": false, "rtl": false, "font": "Arial", "size": 14, "media": [], "id": 6},
                    {"name": "Metadata", "ord": 6, "sticky": false, "rtl": false, "font": "Arial", "size": 20, "media": [], "id": 7}
                ],
                "tmpls": [{
                    "name": "Card 1",
                    "ord": 0,
                    "qfmt": MODEL_QFMT,
                    "afmt": MODEL_AFMT,
                    "did": null,
                    "bqfmt": "",
                    "bafmt": ""
                }],
                "tags": [],
                "did": deck_id,
                "usn": -1,
                "req": [[0, "all", [0]]]
            }
        });

        let conf = serde_json::json!({
            "activeDecks": [deck_id],
            "curDeck": deck_id,
            "newSpread": 0,
            "collapseTime": 1200,
            "timeLim": 0,
            "estTimes": true,
            "dueCounts": true,
            "curModel": model_id,
            "nextPos": 1,
            "sortType": "noteFld",
            "sortBackwards": false,
            "addToCur": true
        });

        let dconf = serde_json::json!({
            "1": {
                "id": 1,
                "name": "Default",
                "mod": 0,
                "usn": 0,
                "maxTaken": 60,
                "autoplay": true,
                "timer": 0,
                "replayq": true,
                "new": {"delays": [1, 10], "ints": [1, 4, 7], "initialFactor": 2500, "order": 1, "perDay": 20},
                "rev": {"perDay": 200, "ease4": 1.3, "fuzz": 0.05, "minSpace": 1, "ivlFct": 1.0, "maxIvl": 36500},
                "lapse": {"delays": [10], "mult": 0.0, "minInt": 1, "leechFails": 8, "leechAction": 0},
                "dyn": false
            }
        });

        conn.execute(
            "INSERT INTO col (id, crt, mod, scm, ver, dty, usn, ls, conf, models, decks, dconf, tags) \
             VALUES (1, ?1, ?2, ?3, 11, 0, -1, 0, ?4, ?5, ?6, ?7, '{}')",
            rusqlite::params![
                now,
                now,
                now * 1000,
                conf.to_string(),
                models.to_string(),
                decks.to_string(),
                dconf.to_string(),
            ],
        )?;

        Ok(())
    }

    fn insert_note_type(
        &self,
        _conn: &rusqlite::Connection,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Note type is already embedded in the col table's models JSON
        Ok(())
    }

    fn insert_cards(
        &self,
        conn: &rusqlite::Connection,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;

        let model_id: i64 = 1_400_000_000_000;
        let deck_id: i64 = {
            use std::hash::{DefaultHasher, Hash, Hasher};
            let mut h = DefaultHasher::new();
            self.deck_data.name.hash(&mut h);
            (h.finish() % 1_000_000_000_000 + 2) as i64
        };

        for (i, entry) in self.deck_data.cards.iter().enumerate() {
            let note_id = now * 1000 + i as i64;
            let card_id = note_id + 1;

            // Format audio as Anki sound tag: [sound:filename.mp3]
            let audio_val = entry.audio_path.as_deref()
                .map(|p| {
                    let filename = Path::new(p)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("audio.mp3");
                    format!("[sound:{}]", filename)
                })
                .unwrap_or_default();

            // Fields separated by \x1f — order must match flds array in models:
            // Front, Back, SkillName, IPA, Audio, Explanation, Metadata
            let flds = format!("{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}",
                entry.front_html, entry.back_html, entry.skill_name,
                entry.ipa, audio_val, entry.explanation, entry.metadata_json
            );

            let csum = Self::field_checksum(&entry.front_html);
            let guid = format!("lc_{}", note_id);
            
            // Adding a space-padded tag string which Anki SQLite requires for exact matching
            let tags = format!(" LC_Version:{} ", env!("CARGO_PKG_VERSION"));

            conn.execute(
                "INSERT INTO notes (id, guid, mid, mod, usn, tags, flds, sfld, csum, flags, data) \
                 VALUES (?1, ?2, ?3, ?4, -1, ?5, ?6, ?7, ?8, 0, '')",
                rusqlite::params![note_id, guid, model_id, now, tags, flds, entry.front_html, csum],
            )?;

            conn.execute(
                "INSERT INTO cards (id, nid, did, ord, mod, usn, type, queue, due, ivl, factor, reps, lapses, left, odue, odid, flags, data) \
                 VALUES (?1, ?2, ?3, 0, ?4, -1, 0, 0, ?5, 0, 0, 0, 0, 0, 0, 0, 0, '')",
                rusqlite::params![card_id, note_id, deck_id, now, i as i64],
            )?;
        }

        Ok(())
    }

    pub(crate) fn field_checksum(field: &str) -> i64 {
        use std::hash::{DefaultHasher, Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        field.hash(&mut hasher);
        (hasher.finish() % 1_000_000_000) as i64
    }

    pub(crate) fn serialize_db(
        conn: &rusqlite::Connection,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Use VACUUM INTO to serialize the in-memory DB
        // Use a unique filename to avoid race conditions in parallel tests
        let unique_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let tmp_path = std::env::temp_dir().join(format!(
            "panglot_{}_{}.anki2",
            std::process::id(),
            unique_id
        ));
        let tmp_str = tmp_path.to_str().ok_or("Invalid temp path")?;

        // Remove if already exists
        let _ = fs::remove_file(&tmp_path);

        let escaped = tmp_str.replace('\'', "''");
        conn.execute(&format!("VACUUM INTO '{}'", escaped), [])?;
        let bytes = fs::read(&tmp_path)?;
        let _ = fs::remove_file(&tmp_path);

        Ok(bytes)
    }
}

// ----- MultiDeckBuilder -----

/// Exports multiple decks into a single `.apkg`, preserving the `::` hierarchy.
/// Each `NewDeckData.name` should be a full path like "Polish::Grammar::Cases".
/// Anki will automatically create the nested deck structure on import.
pub struct MultiDeckBuilder {
    pub decks: Vec<NewDeckData>,
}

impl MultiDeckBuilder {
    pub fn new(decks: Vec<NewDeckData>) -> Self {
        Self { decks }
    }

    pub fn total_cards(&self) -> usize {
        self.decks.iter().map(|d| d.cards.len()).sum()
    }

    /// Generates a stable deck_id from a deck name, avoiding Anki reserved ids 0 and 1.
    fn deck_id_for_name(name: &str) -> i64 {
        use std::hash::{DefaultHasher, Hash, Hasher};
        let mut h = DefaultHasher::new();
        name.hash(&mut h);
        (h.finish() % 1_000_000_000_000 + 2) as i64
    }

    /// Collects all deck paths including intermediate parent decks.
    /// E.g. "A::B::C" produces ["A", "A::B", "A::B::C"].
    fn all_deck_paths(&self) -> Vec<String> {
        let mut paths = std::collections::BTreeSet::new();
        for deck in &self.decks {
            let parts: Vec<&str> = deck.name.split("::").collect();
            for i in 1..=parts.len() {
                paths.insert(parts[..i].join("::"));
            }
        }
        paths.into_iter().collect()
    }

    pub fn export_apkg(&self, path: impl AsRef<Path>) -> Result<(), Box<dyn std::error::Error>> {
        let path = path.as_ref();

        let conn = rusqlite::Connection::open_in_memory()?;
        // Reuse DeckBuilder's schema creation (same Anki schema)
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS col (
                id INTEGER PRIMARY KEY, crt INTEGER NOT NULL, mod INTEGER NOT NULL,
                scm INTEGER NOT NULL, ver INTEGER NOT NULL, dty INTEGER NOT NULL,
                usn INTEGER NOT NULL, ls INTEGER NOT NULL, conf TEXT NOT NULL,
                models TEXT NOT NULL, decks TEXT NOT NULL, dconf TEXT NOT NULL, tags TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS notes (
                id INTEGER PRIMARY KEY, guid TEXT NOT NULL, mid INTEGER NOT NULL,
                mod INTEGER NOT NULL, usn INTEGER NOT NULL, tags TEXT NOT NULL,
                flds TEXT NOT NULL, sfld TEXT NOT NULL, csum INTEGER NOT NULL,
                flags INTEGER NOT NULL, data TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS cards (
                id INTEGER PRIMARY KEY, nid INTEGER NOT NULL, did INTEGER NOT NULL,
                ord INTEGER NOT NULL, mod INTEGER NOT NULL, usn INTEGER NOT NULL,
                type INTEGER NOT NULL, queue INTEGER NOT NULL, due INTEGER NOT NULL,
                ivl INTEGER NOT NULL, factor INTEGER NOT NULL, reps INTEGER NOT NULL,
                lapses INTEGER NOT NULL, left INTEGER NOT NULL, odue INTEGER NOT NULL,
                odid INTEGER NOT NULL, flags INTEGER NOT NULL, data TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS revlog (
                id INTEGER PRIMARY KEY, cid INTEGER NOT NULL, usn INTEGER NOT NULL,
                ease INTEGER NOT NULL, ivl INTEGER NOT NULL, lastIvl INTEGER NOT NULL,
                factor INTEGER NOT NULL, time INTEGER NOT NULL, type INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS graves (
                usn INTEGER NOT NULL, oid INTEGER NOT NULL, type INTEGER NOT NULL
            );"
        )?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;

        // Build deck name -> id mapping (including all intermediate parents)
        let all_paths = self.all_deck_paths();
        let deck_ids: HashMap<String, i64> = all_paths.iter()
            .map(|p| (p.clone(), Self::deck_id_for_name(p)))
            .collect();

        // Build the decks JSON for Anki's col table
        let mut decks_json = serde_json::Map::new();
        for (name, &id) in &deck_ids {
            decks_json.insert(id.to_string(), serde_json::json!({
                "id": id, "name": name, "mod": now, "usn": -1,
                "lrnToday": [0, 0], "revToday": [0, 0], "newToday": [0, 0],
                "timeToday": [0, 0], "collapsed": false, "desc": "",
                "dyn": 0, "conf": 1, "extendRev": 0, "extendNew": 0
            }));
        }

        let first_deck_id = deck_ids.values().next().copied().unwrap_or(2);
        let model_id: i64 = 1_400_000_000_000;

        let models = serde_json::json!({
            model_id.to_string(): {
                "id": model_id, "name": "Panglot", "mod": now, "type": 0,
                "sortf": 0, "vers": [],
                "latexPre": "\\documentclass[12pt]{article}\n\\special{papersize=3in,5in}\n\\usepackage{amssymb,amsmath}\n\\pagestyle{empty}\n\\setlength{\\parindent}{0in}\n\\begin{document}\n",
                "latexPost": "\\end{document}", "latexsvg": false,
                "css": MODEL_CSS,
                "flds": [
                    {"name": "Front", "ord": 0, "sticky": false, "rtl": false, "font": "Arial", "size": 20, "media": [], "id": 1},
                    {"name": "Back", "ord": 1, "sticky": false, "rtl": false, "font": "Arial", "size": 20, "media": [], "id": 2},
                    {"name": "SkillName", "ord": 2, "sticky": false, "rtl": false, "font": "Arial", "size": 20, "media": [], "id": 3},
                    {"name": "IPA", "ord": 3, "sticky": false, "rtl": false, "font": "Arial", "size": 20, "media": [], "id": 4},
                    {"name": "Audio", "ord": 4, "sticky": false, "rtl": false, "font": "Arial", "size": 20, "media": [], "id": 5},
                    {"name": "Explanation", "ord": 5, "sticky": false, "rtl": false, "font": "Arial", "size": 14, "media": [], "id": 6},
                    {"name": "Metadata", "ord": 6, "sticky": false, "rtl": false, "font": "Arial", "size": 20, "media": [], "id": 7}
                ],
                "tmpls": [{"name": "Card 1", "ord": 0, "qfmt": MODEL_QFMT, "afmt": MODEL_AFMT, "did": null, "bqfmt": "", "bafmt": ""}],
                "tags": [], "did": first_deck_id, "usn": -1, "req": [[0, "all", [0]]]
            }
        });

        let conf = serde_json::json!({
            "activeDecks": [first_deck_id], "curDeck": first_deck_id,
            "newSpread": 0, "collapseTime": 1200, "timeLim": 0,
            "estTimes": true, "dueCounts": true, "curModel": model_id,
            "nextPos": 1, "sortType": "noteFld", "sortBackwards": false, "addToCur": true
        });

        let dconf = serde_json::json!({
            "1": {
                "id": 1, "name": "Default", "mod": 0, "usn": 0, "maxTaken": 60,
                "autoplay": true, "timer": 0, "replayq": true,
                "new": {"delays": [1, 10], "ints": [1, 4, 7], "initialFactor": 2500, "order": 1, "perDay": 20},
                "rev": {"perDay": 200, "ease4": 1.3, "fuzz": 0.05, "minSpace": 1, "ivlFct": 1.0, "maxIvl": 36500},
                "lapse": {"delays": [10], "mult": 0.0, "minInt": 1, "leechFails": 8, "leechAction": 0},
                "dyn": false
            }
        });

        conn.execute(
            "INSERT INTO col (id, crt, mod, scm, ver, dty, usn, ls, conf, models, decks, dconf, tags) \
             VALUES (1, ?1, ?2, ?3, 11, 0, -1, 0, ?4, ?5, ?6, ?7, '{}')",
            rusqlite::params![
                now, now, now * 1000,
                conf.to_string(),
                models.to_string(),
                serde_json::Value::Object(decks_json).to_string(),
                dconf.to_string(),
            ],
        )?;

        // Insert notes and cards, assigning each to the correct deck_id
        let tags = format!(" LC_Version:{} ", env!("CARGO_PKG_VERSION"));
        let mut global_idx: usize = 0;

        for deck in &self.decks {
            let did = deck_ids.get(&deck.name).copied().unwrap_or(first_deck_id);

            for entry in &deck.cards {
                let note_id = now * 1000 + global_idx as i64;
                let card_id = note_id + 1;

                let audio_val = entry.audio_path.as_deref()
                    .map(|p| {
                        let filename = Path::new(p).file_name().and_then(|n| n.to_str()).unwrap_or("audio.mp3");
                        format!("[sound:{}]", filename)
                    })
                    .unwrap_or_default();

                let flds = format!("{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}",
                    entry.front_html, entry.back_html, entry.skill_name,
                    entry.ipa, audio_val, entry.explanation, entry.metadata_json
                );

                let csum = DeckBuilder::field_checksum(&entry.front_html);
                let guid = format!("lc_{}", note_id);

                conn.execute(
                    "INSERT INTO notes (id, guid, mid, mod, usn, tags, flds, sfld, csum, flags, data) \
                     VALUES (?1, ?2, ?3, ?4, -1, ?5, ?6, ?7, ?8, 0, '')",
                    rusqlite::params![note_id, guid, model_id, now, tags, flds, entry.front_html, csum],
                )?;

                conn.execute(
                    "INSERT INTO cards (id, nid, did, ord, mod, usn, type, queue, due, ivl, factor, reps, lapses, left, odue, odid, flags, data) \
                     VALUES (?1, ?2, ?3, 0, ?4, -1, 0, 0, ?5, 0, 0, 0, 0, 0, 0, 0, 0, '')",
                    rusqlite::params![card_id, note_id, did, now, global_idx as i64],
                )?;

                global_idx += 1;
            }
        }

        // Serialize DB and package into ZIP
        let db_bytes = DeckBuilder::serialize_db(&conn)?;
        let file = fs::File::create(path)?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        zip.start_file("collection.anki2", options)?;
        zip.write_all(&db_bytes)?;

        // Package audio media
        let mut media_map: HashMap<String, String> = HashMap::new();
        let mut media_index: usize = 0;

        for deck in &self.decks {
            for entry in &deck.cards {
                if let Some(audio_path) = &entry.audio_path {
                    let file_path = Path::new(audio_path);
                    if file_path.exists() {
                        let filename = file_path.file_name().and_then(|n| n.to_str())
                            .unwrap_or("audio.mp3").to_string();
                        let audio_bytes = fs::read(file_path)?;
                        let index_str = media_index.to_string();
                        zip.start_file(&index_str, options)?;
                        zip.write_all(&audio_bytes)?;
                        media_map.insert(index_str, filename);
                        media_index += 1;
                    }
                }
            }
        }

        let media_json = serde_json::to_string(&media_map)?;
        zip.start_file("media", options)?;
        zip.write_all(media_json.as_bytes())?;

        zip.finish()?;
        Ok(())
    }
}

// ----- Tests -----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_apkg_creates_file() {
        let deck_data = NewDeckData {
            name: "Export Test".to_string(),
            language_code: "pol".to_string(),
            cards: vec![
                NewCardEntry {
                    front_html: "Test front".to_string(),
                    back_html: "Test back".to_string(),
                    skill_name: "Grammar".to_string(),
                    template_name: "default".to_string(),
                    fields_json: "{}".to_string(),
                    explanation: "Test exp".to_string(),
                    ipa: "".to_string(),
                    metadata_json: "{}".to_string(),
                    audio_path: None,
                }
            ],
        };

        let builder = DeckBuilder::new(deck_data);
        let tmp_path = std::env::temp_dir().join("test_export.apkg");
        builder.export_apkg(&tmp_path).unwrap();

        assert!(tmp_path.exists());
        // Verify it's a valid ZIP
        let file = fs::File::open(&tmp_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        assert!(archive.by_name("collection.anki2").is_ok());
        assert!(archive.by_name("media").is_ok());

        // Cleanup
        let _ = fs::remove_file(&tmp_path);
    }

    #[test]
    fn export_apkg_contains_valid_sqlite() {
        let deck_data = NewDeckData {
            name: "SQLite Test".to_string(),
            language_code: "pol".to_string(),
            cards: vec![
                NewCardEntry {
                    front_html: "hello".to_string(),
                    back_html: "hola".to_string(),
                    skill_name: "s1".to_string(),
                    template_name: "default".to_string(),
                    fields_json: "{}".to_string(),
                    explanation: "exp".to_string(),
                    ipa: "he'lo".to_string(),
                    metadata_json: "{}".to_string(),
                    audio_path: None,
                }
            ],
        };

        let builder = DeckBuilder::new(deck_data);
        let tmp_path = std::env::temp_dir().join("test_sqlite.apkg");
        builder.export_apkg(&tmp_path).unwrap();

        let file = fs::File::open(&tmp_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let mut sqlite_file = archive.by_name("collection.anki2").unwrap();

        let db_path = std::env::temp_dir().join("extracted.anki2");
        let mut out = fs::File::create(&db_path).unwrap();
        std::io::copy(&mut sqlite_file, &mut out).unwrap();

        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Check decks
        let deck_json: String = conn.query_row("SELECT decks FROM col LIMIT 1", [], |row| row.get(0)).unwrap();
        assert!(deck_json.contains("SQLite Test"));

        // Check cards & notes
        let note_count: i64 = conn.query_row("SELECT count(*) FROM notes", [], |row| row.get(0)).unwrap();
        assert_eq!(note_count, 1);

        let front: String = conn.query_row("SELECT sfld FROM notes LIMIT 1", [], |row| row.get(0)).unwrap();
        assert_eq!(front, "hello");

        // Cleanup
        let _ = fs::remove_file(&tmp_path);
        let _ = fs::remove_file(&db_path);
    }
}
