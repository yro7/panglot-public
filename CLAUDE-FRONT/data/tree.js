// Skill tree data for German (de-target)
// `name` is the target-language (German) display string — may be overridden by backend.
// Dynamic fields (mastery, status, exercisesDone) default to fresh; backend merges live values.

window.PANGLOT_TREE = {
  language: { code: "de", native: "Deutsch", flag: "DE" },

  userFrontier: 0,

  categories: [
    { id: "phonetics",   label: { fr: "Phonétique",  en: "Phonetics",   de: "Phonetik" },     hue: 28  },
    { id: "grammar",     label: { fr: "Grammaire",   en: "Grammar",     de: "Grammatik" },    hue: 255 },
    { id: "vocabulary",  label: { fr: "Lexique",     en: "Vocabulary",  de: "Wortschatz" },   hue: 155 },
    { id: "conjugation", label: { fr: "Conjugaison", en: "Conjugation", de: "Konjugation" },  hue: 345 },
    { id: "syntax",      label: { fr: "Syntaxe",     en: "Syntax",      de: "Syntax" },       hue: 75  }
  ],
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
      name: "Alphabet & Umlaute",
      native: "ä ö ü ß",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: [], decks: ["Voyelles longues", "Le ß", "Diphtongues"] },

    { id: "pronouns-personal", category: "grammar", palier: 0, sub: 0,
      name: "Personalpronomen",
      native: "ich · du · er · sie · es",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: [], decks: ["Sujet au nominatif", "Tu vs vous (du/Sie)"] },

    { id: "nouns-basic", category: "vocabulary", palier: 0, sub: 0,
      name: "Grundwortschatz",
      native: "Haus · Frau · Buch",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: [], decks: ["100 noms quotidiens", "Objets de la maison", "Famille"] },

    // --- A1.2 ---
    { id: "articles-def", category: "grammar", palier: 0, sub: 1,
      name: "Bestimmte Artikel",
      native: "der · die · das",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["pronouns-personal"], decks: ["Genre des noms", "Pluriel des articles", "Accord article-nom"] },

    { id: "verbs-sein-haben", category: "conjugation", palier: 0, sub: 1,
      name: "sein & haben",
      native: "ich bin · ich habe",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["pronouns-personal"], decks: ["sein au présent", "haben au présent", "Questions avec sein"] },

    // --- A1.3 ---
    { id: "articles-indef", category: "grammar", palier: 0, sub: 2,
      name: "Unbestimmte Artikel",
      native: "ein · eine · ein",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["articles-def"], decks: ["ein/eine/ein", "Négation kein"] },

    { id: "word-order-basic", category: "syntax", palier: 0, sub: 2,
      name: "Wortstellung (Basis)",
      native: "S – V – O",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["verbs-sein-haben"], decks: ["Phrase déclarative", "Question sans mot interrogatif"] },

    // --- A2.1 ---
    { id: "numbers", category: "vocabulary", palier: 1, sub: 0,
      name: "Zahlen & Uhrzeit",
      native: "eins · zwei · drei",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["nouns-basic"], decks: ["0 → 100", "Heures", "Dates"] },

    { id: "verbs-regular", category: "conjugation", palier: 1, sub: 0,
      name: "Regelmäßige Verben",
      native: "machen · lernen · spielen",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["verbs-sein-haben"], decks: ["Présent", "Terminaisons -en", "50 verbes fréquents"] },

    // --- A2.2 ---
    { id: "nom-akk", category: "grammar", palier: 1, sub: 1,
      name: "Nominativ & Akkusativ",
      native: "der → den",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["articles-def", "articles-indef"], decks: ["COD au masculin", "Verbes à accusatif", "Pronoms à l'accusatif"] },

    { id: "phonetics-stress", category: "phonetics", palier: 1, sub: 1,
      name: "Wortakzent",
      native: "Betonung",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["alphabet"], decks: ["Mots composés", "Préfixes séparables"] },

    { id: "questions-w", category: "syntax", palier: 1, sub: 1,
      name: "W-Fragen",
      native: "Wer · Was · Wo · Wann",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["word-order-basic"], decks: ["W-Fragen essentielles", "Poser des questions"] },

    // --- A2.3 ---
    { id: "possessives", category: "grammar", palier: 1, sub: 2,
      name: "Possessivpronomen",
      native: "mein · dein · sein",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["nom-akk"], decks: ["mein/meine", "Déclinaison du possessif"] },

    { id: "separable-verbs", category: "conjugation", palier: 1, sub: 2,
      name: "Trennbare Verben",
      native: "aufstehen · ankommen",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["verbs-regular"], decks: ["Préfixes séparables", "Position finale de la particule"] },

    // --- B1.1 ---
    { id: "dativ", category: "grammar", palier: 2, sub: 0,
      name: "Dativ",
      native: "dem · der · dem",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["nom-akk"], decks: ["Verbes à datif", "Prépositions + datif", "Pronoms datifs"] },

    { id: "modals", category: "conjugation", palier: 2, sub: 0,
      name: "Modalverben",
      native: "können · müssen · dürfen",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["verbs-regular"], decks: ["können au présent", "müssen/sollen", "Ordre des mots modal"] },

    // --- B1.2 ---
    { id: "perfect", category: "conjugation", palier: 2, sub: 1,
      name: "Perfekt",
      native: "ich habe gemacht",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["verbs-regular"], decks: ["haben + participe", "sein + participe", "Participes irréguliers"] },

    { id: "subord-clause", category: "syntax", palier: 2, sub: 1,
      name: "Nebensätze",
      native: "…, weil ich müde bin",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["questions-w", "modals"], decks: ["weil / dass", "Verbe en fin", "Conjonctions de subordination"] },

    // --- B1.3 ---
    { id: "prepositions-wechsel", category: "grammar", palier: 2, sub: 2,
      name: "Wechselpräpositionen",
      native: "in · an · auf",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["dativ"], decks: ["Accusatif vs datif", "Lieu vs direction"] },

    // --- B2.1 ---
    { id: "genitiv", category: "grammar", palier: 3, sub: 0,
      name: "Genitiv",
      native: "des Hauses",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["dativ"], decks: ["Prépositions + génitif", "Noms propres au génitif"] },

    { id: "preterit", category: "conjugation", palier: 3, sub: 0,
      name: "Präteritum",
      native: "ich machte · ich ging",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["perfect"], decks: ["Réguliers", "Irréguliers forts", "Narration écrite"] },

    // --- B2.2 ---
    { id: "adj-declension", category: "grammar", palier: 3, sub: 1,
      name: "Adjektivdeklination",
      native: "der gute Mann",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["dativ", "possessives"], decks: ["Déclinaison faible", "Déclinaison mixte", "Déclinaison forte"] },

    { id: "passive", category: "syntax", palier: 3, sub: 1,
      name: "Passiv",
      native: "wird gemacht",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["perfect", "subord-clause"], decks: ["Passif processus", "Passif d'état", "Agent avec von/durch"] },

    // --- B2.3 ---
    { id: "relative-clauses", category: "syntax", palier: 3, sub: 2,
      name: "Relativsätze",
      native: "…, der …, die …",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["subord-clause"], decks: ["Pronoms relatifs", "Relatives au datif"] },

    // --- C1.1 ---
    { id: "konjunktiv-ii", category: "conjugation", palier: 4, sub: 0,
      name: "Konjunktiv II",
      native: "wäre · hätte · würde",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["preterit"], decks: ["Irréel du présent", "Politesse", "würde + infinitif"] },

    // --- C1.2 ---
    { id: "nominalization", category: "vocabulary", palier: 4, sub: 1,
      name: "Nominalisierung",
      native: "das Schreiben",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["adj-declension"], decks: ["Infinitif substantivé", "Registre écrit"] },

    { id: "passiv-advanced", category: "syntax", palier: 4, sub: 1,
      name: "Passiv-Ersatzformen",
      native: "lässt sich · ist zu tun",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["passive"], decks: ["sich lassen", "sein + zu + infinitif"] },

    // --- C1.3 ---
    { id: "idioms", category: "vocabulary", palier: 4, sub: 2,
      name: "Redewendungen",
      native: "Daumen drücken",
      mastery: 0, status: "fresh", exercisesDone: 0, lastPracticed: null,
      prereq: ["nominalization"], decks: ["Idiomes courants", "Proverbes", "Registre familier"] }
  ]
};

// Helper: absolute level value = palier + sub/3
window.PANGLOT_TREE.nodeLevel = function(n) { return n.palier + n.sub / 3; };
