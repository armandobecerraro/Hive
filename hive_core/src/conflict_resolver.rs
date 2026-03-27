//! Resolución automática de conflictos de merge.
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ConflictResolution {
    pub file: String,
    pub resolved: bool,
    pub resolution: String,
}

pub struct ConflictResolver;

impl ConflictResolver {
    /// Intenta resolver conflictos automáticamente.
    pub fn auto_resolve(content: &str) -> (String, Vec<ConflictResolution>) {
        let mut result = content.to_string();
        let mut resolutions = Vec::new();

        while let Some(start) = result.find("<<<<<<<") {
            if let Some(mid) = result.find("=======") {
                if let Some(end) = result.find(">>>>>>>") {
                    let ours = result[start + 7..mid].trim().to_string();
                    let _theirs = result[mid + 7..end + 7].to_string();
                    // Estrategia simple: quedarse con ours
                    result = format!("{}{}{}", &result[..start], ours, &result[end + 7..]);
                    resolutions.push(ConflictResolution {
                        file: String::new(),
                        resolved: true,
                        resolution: "kept ours".into(),
                    });
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        (result, resolutions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_resolve_basic() {
        let conflict = "<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> branch\n";
        let (resolved, resolutions) = ConflictResolver::auto_resolve(conflict);
        assert!(!resolved.contains("<<<<<<<"));
        assert!(!resolutions.is_empty());
    }
}
