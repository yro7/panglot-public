use iso15924::ScriptCode;
use isolang::Language;

fn main() {
    // ISO 15924 — Script codes
    let script = ScriptCode::by_code("Latn");
    println!("Latn: {:?}", script);

    let script_by_num = ScriptCode::by_num("215");
    println!("215: {:?}", script_by_num);

    // ISO 639-3 — Language codes
    let lang = Language::Pol;
    println!("Polish: {} -> {}", lang.to_639_3(), lang.to_name());

    let lang = Language::Jpn;
    println!("Japanese: {} -> {}", lang.to_639_3(), lang.to_name());
}
