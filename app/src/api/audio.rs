use actix_web::{web, HttpResponse};

/// Serves an audio file from the TTS staging directory.
/// GET /api/audio/{filename}
pub async fn get_audio(_auth: crate::auth::AuthUser, filename: web::Path<String>) -> HttpResponse {
    let filename = filename.into_inner();

    // Sanitize: only allow simple filenames (no path traversal, null bytes)
    if filename.contains('/') || filename.contains('\\') || filename.contains("..") || filename.contains('\0') {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid filename"}));
    }
    if filename.len() > 200 {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "Filename too long"}));
    }
    if !matches!(filename.rsplit('.').next(), Some("mp3" | "wav" | "ogg")) {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "Unsupported audio format"}));
    }

    let staging_dir = std::env::temp_dir().join("lc_audio");
    let file_path = staging_dir.join(&filename);

    match std::fs::read(&file_path) {
        Ok(bytes) => {
            let content_type = if filename.ends_with(".mp3") {
                "audio/mpeg"
            } else if filename.ends_with(".wav") {
                "audio/wav"
            } else if filename.ends_with(".ogg") {
                "audio/ogg"
            } else {
                "application/octet-stream"
            };

            HttpResponse::Ok()
                .content_type(content_type)
                .append_header(("Cache-Control", "public, max-age=86400"))
                .body(bytes)
        }
        Err(_) => HttpResponse::NotFound().json(serde_json::json!({"error": "Audio file not found"})),
    }
}
