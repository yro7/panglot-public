use isolang::Language;

fn main() {
    // Test Language::from_name() to look up by English name
    if let Some(lang) = Language::from_name("English") {
        println!("English -> {} -> {}", lang.to_639_3(), lang.to_name());
    } else {
        println!("English not found");
    }
    
    if let Some(lang) = Language::from_name("French") {
        println!("French -> {} -> {}", lang.to_639_3(), lang.to_name());
    } else {
        println!("French not found");
    }
    
    if let Some(lang) = Language::from_name("Polish") {
        println!("Polish -> {} -> {}", lang.to_639_3(), lang.to_name());
    } else {
        println!("Polish not found");
    }
}
