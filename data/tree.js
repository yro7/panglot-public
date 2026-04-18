// Skill tree data for German (de-target)
// Levels are now fractional: level.tier (A1 = 0, A1.5 = 0.5, A2 = 1, etc.)
// Each node has a "subLevel" (0..2) within its palier to create micro-columns.
// No more "locked" — all nodes are accessible. `status` is mastered / in_progress / fresh.
// The "zone de confort" is computed from the user's frontier (median of in_progress nodes).

window.PANGLOT_TREE = {
  language: { code: "de", native: "Deutsch", flag: "DE" },

  // User's current frontier — nodes close to this value are "in the zone"
  // Below = review, above = ambitious, far above = way beyond
  userFrontier: 1.4, // between A2 and B1

  categories: [
    { id: "phonetics",   label: { fr: "Phonétique",  en: "Phonetics",   de: "Phonetik" },     hue: 28  },
    { id: "grammar",     label: { fr: "Grammaire",   en: "Grammar",     de: "Grammatik" },    hue: 255 },
    { id: "vocabulary",  label: { fr: "Lexique",     en: "Vocabulary",  de: "Wortschatz" },   hue: 155 },
    { id: "conjugation", label: { fr: "Conjugaison", en: "Conjugation", de: "Konjugation" },  hue: 345 },
    { id: "syntax",      label: { fr: "Syntaxe",     en: "Syntax",      de: "Syntax" },       hue: 75  }
  ],
  // Paliers with sub-echelons (3 tiers per palier)
  paliers: [
    { id: "a1", label: { fr: "A1 · Débutant",      en: "A1 · Beginner",    de: "A1 · Anfänger" } },
    { id: "a2", label: { fr: "A2 · Élémentaire",   en: "A2 · Elementary",  de: "A2 · Grundstufe" } },
    { id: "b1", label: { fr: "B1 · Intermédiaire", en: "B1 · Intermediate",de: "B1 · Mittelstufe" } },
    { id: "b2", label: { fr: "B2 · Avancé",        en: "B2 · Upper-int.",  de: "B2 · Fortgeschr." } },
    { id: "c1", label: { fr: "C1 · Autonome",      en: "C1 · Advanced",    de: "C1 · Sicher" } }
  ],
  nodes: [
    // --- A1.1 ---
    { id: "alphabet", category: "phonetics", palier: 0, sub: 0,
      title: { fr: "Alphabet & umlauts", en: "Alphabet & umlauts", de: "Alphabet & Umlaute" },
      native: "ä ö ü ß",
      status: "mastered", mastery: 1.0, exercisesDone: 42, lastPracticed: 18,
      prereq: [], decks: ["Voyelles longues", "Le ß", "Diphtongues"] },

    { id: "pronouns-personal", category: "grammar", palier: 0, sub: 0,
      title: { fr: "Pronoms personnels", en: "Personal pronouns", de: "Personalpronomen" },
      native: "ich · du · er · sie · es",
      status: "mastered", mastery: 1.0, exercisesDone: 38, lastPracticed: 14,
      prereq: [], decks: ["Sujet au nominatif", "Tu vs vous (du/Sie)"] },

    { id: "nouns-basic", category: "vocabulary", palier: 0, sub: 0,
      title: { fr: "Noms essentiels", en: "Core nouns", de: "Grundwortschatz" },
      native: "Haus · Frau · Buch",
      status: "mastered", mastery: 1.0, exercisesDone: 120, lastPracticed: 3,
      prereq: [], decks: ["100 noms quotidiens", "Objets de la maison", "Famille"] },

    // --- A1.2 ---
    { id: "articles-def", category: "grammar", palier: 0, sub: 1,
      title: { fr: "Articles définis", en: "Definite articles", de: "Bestimmte Artikel" },
      native: "der · die · das",
      status: "mastered", mastery: 0.95, exercisesDone: 54, lastPracticed: 7,
      prereq: ["pronouns-personal"], decks: ["Genre des noms", "Pluriel des articles", "Accord article-nom"] },

    { id: "verbs-sein-haben", category: "conjugation", palier: 0, sub: 1,
      title: { fr: "sein & haben", en: "to be & to have", de: "sein & haben" },
      native: "ich bin · ich habe",
      status: "mastered", mastery: 1.0, exercisesDone: 67, lastPracticed: 5,
      prereq: ["pronouns-personal"], decks: ["sein au présent", "haben au présent", "Questions avec sein"] },

    // --- A1.3 ---
    { id: "articles-indef", category: "grammar", palier: 0, sub: 2,
      title: { fr: "Articles indéfinis", en: "Indefinite articles", de: "Unbestimmte Artikel" },
      native: "ein · eine · ein",
      status: "mastered", mastery: 0.88, exercisesDone: 31, lastPracticed: 9,
      prereq: ["articles-def"], decks: ["ein/eine/ein", "Négation kein"] },

    { id: "word-order-basic", category: "syntax", palier: 0, sub: 2,
      title: { fr: "Ordre des mots (base)", en: "Basic word order", de: "Wortstellung (Basis)" },
      native: "S – V – O",
      status: "mastered", mastery: 0.9, exercisesDone: 28, lastPracticed: 4,
      prereq: ["verbs-sein-haben"], decks: ["Phrase déclarative", "Question sans mot interrogatif"] },

    // --- A2.1 ---
    { id: "numbers", category: "vocabulary", palier: 1, sub: 0,
      title: { fr: "Nombres & heure", en: "Numbers & time", de: "Zahlen & Uhrzeit" },
      native: "eins · zwei · drei",
      status: "mastered", mastery: 0.92, exercisesDone: 30, lastPracticed: 6,
      prereq: ["nouns-basic"], decks: ["0 → 100", "Heures", "Dates"] },

    { id: "verbs-regular", category: "conjugation", palier: 1, sub: 0,
      title: { fr: "Verbes réguliers", en: "Regular verbs", de: "Regelmäßige Verben" },
      native: "machen · lernen · spielen",
      status: "mastered", mastery: 0.85, exercisesDone: 52, lastPracticed: 2,
      prereq: ["verbs-sein-haben"], decks: ["Présent", "Terminaisons -en", "50 verbes fréquents"] },

    // --- A2.2 ---
    { id: "nom-akk", category: "grammar", palier: 1, sub: 1,
      title: { fr: "Nominatif & Accusatif", en: "Nominative & Accusative", de: "Nominativ & Akkusativ" },
      native: "der → den",
      status: "in_progress", mastery: 0.62, exercisesDone: 44, lastPracticed: 1,
      prereq: ["articles-def", "articles-indef"], decks: ["COD au masculin", "Verbes à accusatif", "Pronoms à l'accusatif"] },

    { id: "phonetics-stress", category: "phonetics", palier: 1, sub: 1,
      title: { fr: "Accentuation", en: "Word stress", de: "Wortakzent" },
      native: "Betonung",
      status: "in_progress", mastery: 0.35, exercisesDone: 8, lastPracticed: 11,
      prereq: ["alphabet"], decks: ["Mots composés", "Préfixes séparables"] },

    { id: "questions-w", category: "syntax", palier: 1, sub: 1,
      title: { fr: "Mots interrogatifs", en: "W-questions", de: "W-Fragen" },
      native: "Wer · Was · Wo · Wann",
      status: "in_progress", mastery: 0.55, exercisesDone: 21, lastPracticed: 2,
      prereq: ["word-order-basic"], decks: ["W-Fragen essentielles", "Poser des questions"] },

    // --- A2.3 ---
    { id: "possessives", category: "grammar", palier: 1, sub: 2,
      title: { fr: "Possessifs", en: "Possessives", de: "Possessivpronomen" },
      native: "mein · dein · sein",
      status: "in_progress", mastery: 0.3, exercisesDone: 9, lastPracticed: 8,
      prereq: ["nom-akk"], decks: ["mein/meine", "Déclinaison du possessif"] },

    { id: "separable-verbs", category: "conjugation", palier: 1, sub: 2,
      title: { fr: "Verbes à particule", en: "Separable verbs", de: "Trennbare Verben" },
      native: "aufstehen · ankommen",
      status: "fresh", mastery: 0.05, exercisesDone: 1, lastPracticed: 22,
      prereq: ["verbs-regular"], decks: ["Préfixes séparables", "Position finale de la particule"] },

    // --- B1.1 ---
    { id: "dativ", category: "grammar", palier: 2, sub: 0,
      title: { fr: "Datif", en: "Dative case", de: "Dativ" },
      native: "dem · der · dem",
      status: "in_progress", mastery: 0.15, exercisesDone: 5, lastPracticed: 3,
      prereq: ["nom-akk"], decks: ["Verbes à datif", "Prépositions + datif", "Pronoms datifs"] },

    { id: "modals", category: "conjugation", palier: 2, sub: 0,
      title: { fr: "Verbes modaux", en: "Modal verbs", de: "Modalverben" },
      native: "können · müssen · dürfen",
      status: "in_progress", mastery: 0.2, exercisesDone: 7, lastPracticed: 10,
      prereq: ["verbs-regular"], decks: ["können au présent", "müssen/sollen", "Ordre des mots modal"] },

    // --- B1.2 ---
    { id: "perfect", category: "conjugation", palier: 2, sub: 1,
      title: { fr: "Parfait", en: "Perfect tense", de: "Perfekt" },
      native: "ich habe gemacht",
      status: "fresh", mastery: 0.08, exercisesDone: 2, lastPracticed: 30,
      prereq: ["verbs-regular"], decks: ["haben + participe", "sein + participe", "Participes irréguliers"] },

    { id: "subord-clause", category: "syntax", palier: 2, sub: 1,
      title: { fr: "Subordonnées", en: "Subordinate clauses", de: "Nebensätze" },
      native: "…, weil ich müde bin",
      status: "fresh", mastery: 0, exercisesDone: 0, lastPracticed: null,
      prereq: ["questions-w", "modals"], decks: ["weil / dass", "Verbe en fin", "Conjonctions de subordination"] },

    // --- B1.3 ---
    { id: "prepositions-wechsel", category: "grammar", palier: 2, sub: 2,
      title: { fr: "Prépositions mixtes", en: "Two-way prepositions", de: "Wechselpräpositionen" },
      native: "in · an · auf",
      status: "fresh", mastery: 0, exercisesDone: 0, lastPracticed: null,
      prereq: ["dativ"], decks: ["Accusatif vs datif", "Lieu vs direction"] },

    // --- B2.1 ---
    { id: "genitiv", category: "grammar", palier: 3, sub: 0,
      title: { fr: "Génitif", en: "Genitive case", de: "Genitiv" },
      native: "des Hauses",
      status: "fresh", mastery: 0, exercisesDone: 0, lastPracticed: null,
      prereq: ["dativ"], decks: ["Prépositions + génitif", "Noms propres au génitif"] },

    { id: "preterit", category: "conjugation", palier: 3, sub: 0,
      title: { fr: "Prétérit", en: "Preterite", de: "Präteritum" },
      native: "ich machte · ich ging",
      status: "fresh", mastery: 0, exercisesDone: 0, lastPracticed: null,
      prereq: ["perfect"], decks: ["Réguliers", "Irréguliers forts", "Narration écrite"] },

    // --- B2.2 ---
    { id: "adj-declension", category: "grammar", palier: 3, sub: 1,
      title: { fr: "Déclinaison adjectif", en: "Adjective endings", de: "Adjektivdeklination" },
      native: "der gute Mann",
      status: "fresh", mastery: 0, exercisesDone: 0, lastPracticed: null,
      prereq: ["dativ", "possessives"], decks: ["Déclinaison faible", "Déclinaison mixte", "Déclinaison forte"] },

    { id: "passive", category: "syntax", palier: 3, sub: 1,
      title: { fr: "Passif", en: "Passive voice", de: "Passiv" },
      native: "wird gemacht",
      status: "fresh", mastery: 0, exercisesDone: 0, lastPracticed: null,
      prereq: ["perfect", "subord-clause"], decks: ["Passif processus", "Passif d'état", "Agent avec von/durch"] },

    // --- B2.3 ---
    { id: "relative-clauses", category: "syntax", palier: 3, sub: 2,
      title: { fr: "Relatives", en: "Relative clauses", de: "Relativsätze" },
      native: "…, der …, die …",
      status: "fresh", mastery: 0, exercisesDone: 0, lastPracticed: null,
      prereq: ["subord-clause"], decks: ["Pronoms relatifs", "Relatives au datif"] },

    // --- C1.1 ---
    { id: "konjunktiv-ii", category: "conjugation", palier: 4, sub: 0,
      title: { fr: "Konjunktiv II", en: "Konjunktiv II", de: "Konjunktiv II" },
      native: "wäre · hätte · würde",
      status: "fresh", mastery: 0, exercisesDone: 0, lastPracticed: null,
      prereq: ["preterit"], decks: ["Irréel du présent", "Politesse", "würde + infinitif"] },

    // --- C1.2 ---
    { id: "nominalization", category: "vocabulary", palier: 4, sub: 1,
      title: { fr: "Nominalisation", en: "Nominalization", de: "Nominalisierung" },
      native: "das Schreiben",
      status: "fresh", mastery: 0, exercisesDone: 0, lastPracticed: null,
      prereq: ["adj-declension"], decks: ["Infinitif substantivé", "Registre écrit"] },

    { id: "passiv-advanced", category: "syntax", palier: 4, sub: 1,
      title: { fr: "Passif & alternatives", en: "Passive & alternatives", de: "Passiv-Ersatzformen" },
      native: "lässt sich · ist zu tun",
      status: "fresh", mastery: 0, exercisesDone: 0, lastPracticed: null,
      prereq: ["passive"], decks: ["sich lassen", "sein + zu + infinitif"] },

    // --- C1.3 ---
    { id: "idioms", category: "vocabulary", palier: 4, sub: 2,
      title: { fr: "Expressions idiomatiques", en: "Idioms", de: "Redewendungen" },
      native: "Daumen drücken",
      status: "fresh", mastery: 0, exercisesDone: 0, lastPracticed: null,
      prereq: ["nominalization"], decks: ["Idiomes courants", "Proverbes", "Registre familier"] }
  ]
};

// Helper: absolute level value = palier + sub/3
window.PANGLOT_TREE.nodeLevel = function(n) { return n.palier + n.sub / 3; };
