//! Fuzzing de parsers y configs.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct FuzzResult {
    pub input_hash: String,
    pub panicked: bool,
    pub error: Option<String>,
}

pub struct Fuzzer;

impl Fuzzer {
    pub fn fuzz_string(len: usize) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        std::time::SystemTime::now().hash(&mut hasher);
        let seed = hasher.finish();
        (0..len)
            .map(|i| ((seed.wrapping_mul(i as u64 + 1)) % 256) as u8 as char)
            .collect()
    }

    pub fn fuzz_json_parse(data: &str) -> FuzzResult {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        use std::hash::{Hash, Hasher};
        data.hash(&mut hasher);
        let result = std::panic::catch_unwind(|| serde_json::from_str::<serde_json::Value>(data));
        match result {
            Ok(Ok(_)) => FuzzResult {
                input_hash: format!("{:x}", hasher.finish()),
                panicked: false,
                error: None,
            },
            Ok(Err(e)) => FuzzResult {
                input_hash: format!("{:x}", hasher.finish()),
                panicked: false,
                error: Some(e.to_string()),
            },
            Err(_) => FuzzResult {
                input_hash: format!("{:x}", hasher.finish()),
                panicked: true,
                error: Some("panic".into()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzz_string_generates() {
        let s = Fuzzer::fuzz_string(100);
        assert!(!s.is_empty(), "fuzz string should not be empty");
    }

    #[test]
    fn fuzz_json_parse_valid() {
        let r = Fuzzer::fuzz_json_parse("{\"key\": \"value\"}");
        assert!(!r.panicked);
    }
}
