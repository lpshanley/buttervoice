use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub family: String,
    pub family_order: u32,
    pub estimated_size_mb: u32,
    pub recommended: bool,
    pub quantized: bool,
}

#[derive(Debug, Clone)]
pub struct ModelDownloadSpec {
    pub id: &'static str,
    pub file_name: &'static str,
    pub download_url: &'static str,
}

pub fn default_model_id() -> &'static str {
    "base.en-q5_1"
}

pub fn available_models() -> Vec<ModelInfo> {
    vec![
        // ── Tiny English (~39M params) ──
        ModelInfo {
            id: "tiny.en-q5_1".to_string(),
            display_name: "Tiny English Q5_1".to_string(),
            family: "Tiny English".to_string(),
            family_order: 0,
            estimated_size_mb: 31,
            recommended: false,
            quantized: true,
        },
        ModelInfo {
            id: "tiny.en".to_string(),
            display_name: "Tiny English".to_string(),
            family: "Tiny English".to_string(),
            family_order: 0,
            estimated_size_mb: 75,
            recommended: false,
            quantized: false,
        },
        // ── Base English (~74M params) ──
        ModelInfo {
            id: "base.en-q5_1".to_string(),
            display_name: "Base English Q5_1".to_string(),
            family: "Base English".to_string(),
            family_order: 1,
            estimated_size_mb: 57,
            recommended: true,
            quantized: true,
        },
        ModelInfo {
            id: "base.en".to_string(),
            display_name: "Base English".to_string(),
            family: "Base English".to_string(),
            family_order: 1,
            estimated_size_mb: 142,
            recommended: false,
            quantized: false,
        },
        // ── Small English (~244M params) ──
        ModelInfo {
            id: "small.en-q5_1".to_string(),
            display_name: "Small English Q5_1".to_string(),
            family: "Small English".to_string(),
            family_order: 2,
            estimated_size_mb: 181,
            recommended: false,
            quantized: true,
        },
        ModelInfo {
            id: "small.en".to_string(),
            display_name: "Small English".to_string(),
            family: "Small English".to_string(),
            family_order: 2,
            estimated_size_mb: 466,
            recommended: false,
            quantized: false,
        },
        // ── Medium English (~769M params) ──
        ModelInfo {
            id: "medium.en".to_string(),
            display_name: "Medium English".to_string(),
            family: "Medium English".to_string(),
            family_order: 3,
            estimated_size_mb: 1533,
            recommended: false,
            quantized: false,
        },
        // ── Large V3 Turbo (~809M params) ──
        ModelInfo {
            id: "large-v3-turbo-q5_0".to_string(),
            display_name: "Large V3 Turbo Q5_0".to_string(),
            family: "Large V3 Turbo".to_string(),
            family_order: 4,
            estimated_size_mb: 547,
            recommended: false,
            quantized: true,
        },
        ModelInfo {
            id: "large-v3-turbo-q8_0".to_string(),
            display_name: "Large V3 Turbo Q8_0".to_string(),
            family: "Large V3 Turbo".to_string(),
            family_order: 4,
            estimated_size_mb: 834,
            recommended: false,
            quantized: true,
        },
        ModelInfo {
            id: "large-v3-turbo".to_string(),
            display_name: "Large V3 Turbo".to_string(),
            family: "Large V3 Turbo".to_string(),
            family_order: 4,
            estimated_size_mb: 1533,
            recommended: false,
            quantized: false,
        },
        // ── Large V3 (~1550M params) ──
        ModelInfo {
            id: "large-v3-q5_0".to_string(),
            display_name: "Large V3 Q5_0".to_string(),
            family: "Large V3".to_string(),
            family_order: 5,
            estimated_size_mb: 1080,
            recommended: false,
            quantized: true,
        },
        ModelInfo {
            id: "large-v3".to_string(),
            display_name: "Large V3".to_string(),
            family: "Large V3".to_string(),
            family_order: 5,
            estimated_size_mb: 3095,
            recommended: false,
            quantized: false,
        },
    ]
}

pub fn download_spec(model_id: &str) -> Option<ModelDownloadSpec> {
    let spec = match model_id {
        "tiny.en-q5_1" => ModelDownloadSpec {
            id: "tiny.en-q5_1",
            file_name: "ggml-tiny.en-q5_1.bin",
            download_url:
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en-q5_1.bin",
        },
        "base.en-q5_1" => ModelDownloadSpec {
            id: "base.en-q5_1",
            file_name: "ggml-base.en-q5_1.bin",
            download_url:
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en-q5_1.bin",
        },
        "small.en-q5_1" => ModelDownloadSpec {
            id: "small.en-q5_1",
            file_name: "ggml-small.en-q5_1.bin",
            download_url:
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en-q5_1.bin",
        },
        "tiny.en" => ModelDownloadSpec {
            id: "tiny.en",
            file_name: "ggml-tiny.en.bin",
            download_url:
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
        },
        "base.en" => ModelDownloadSpec {
            id: "base.en",
            file_name: "ggml-base.en.bin",
            download_url:
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin",
        },
        "small.en" => ModelDownloadSpec {
            id: "small.en",
            file_name: "ggml-small.en.bin",
            download_url:
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
        },
        "medium.en" => ModelDownloadSpec {
            id: "medium.en",
            file_name: "ggml-medium.en.bin",
            download_url:
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin",
        },
        "large-v3-turbo" => ModelDownloadSpec {
            id: "large-v3-turbo",
            file_name: "ggml-large-v3-turbo.bin",
            download_url:
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
        },
        "large-v3-turbo-q5_0" => ModelDownloadSpec {
            id: "large-v3-turbo-q5_0",
            file_name: "ggml-large-v3-turbo-q5_0.bin",
            download_url:
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
        },
        "large-v3-turbo-q8_0" => ModelDownloadSpec {
            id: "large-v3-turbo-q8_0",
            file_name: "ggml-large-v3-turbo-q8_0.bin",
            download_url:
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q8_0.bin",
        },
        "large-v3" => ModelDownloadSpec {
            id: "large-v3",
            file_name: "ggml-large-v3.bin",
            download_url:
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin",
        },
        "large-v3-q5_0" => ModelDownloadSpec {
            id: "large-v3-q5_0",
            file_name: "ggml-large-v3-q5_0.bin",
            download_url:
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-q5_0.bin",
        },
        _ => return None,
    };
    Some(spec)
}
