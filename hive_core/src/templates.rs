//! Templates/presets para stacks comunes.
//! Inspirado en create-react-app y cargo-generate.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub name: String,
    pub description: String,
    pub stack: String,
    pub files: HashMap<String, String>,
    pub post_commands: Vec<String>,
}

pub struct TemplateRegistry {
    templates: Vec<Template>,
}

impl TemplateRegistry {
    pub fn new() -> Self {
        Self {
            templates: Self::default_templates(),
        }
    }

    fn default_templates() -> Vec<Template> {
        vec![
            Template {
                name: "rust-actix-api".into(),
                description: "Actix-web REST API".into(),
                stack: "rust".into(),
                files: HashMap::from([
                    ("Cargo.toml".into(), "[package]\nname = \"api\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nactix-web = \"4\"\ntokio = { version = \"1\", features = [\"full\"] }\nserde = { version = \"1\", features = [\"derive\"] }\n".into()),
                    ("src/main.rs".into(), "use actix_web::{web, App, HttpServer, HttpResponse};\n\nasync fn health() -> HttpResponse {\n    HttpResponse::Ok().body(\"ok\")\n}\n\n#[actix_web::main]\nasync fn main() -> std::io::Result<()> {\n    HttpServer::new(|| App::new().route(\"/health\", web::get().to(health)))\n        .bind(\"127.0.0.1:8080\")?\n        .run()\n        .await\n}\n".into()),
                ]),
                post_commands: vec!["cargo check".into()],
            },
            Template {
                name: "python-fastapi".into(),
                description: "FastAPI REST API".into(),
                stack: "python".into(),
                files: HashMap::from([
                    ("main.py".into(), "from fastapi import FastAPI\n\napp = FastAPI()\n\n@app.get(\"/health\")\ndef health():\n    return {\"status\": \"ok\"}\n".into()),
                    ("requirements.txt".into(), "fastapi\nuvicorn\n".into()),
                ]),
                post_commands: vec!["python3 -m py_compile main.py".into()],
            },
            Template {
                name: "node-express".into(),
                description: "Express.js API".into(),
                stack: "javascript".into(),
                files: HashMap::from([
                    ("index.js".into(), "const express = require('express');\nconst app = express();\n\napp.get('/health', (req, res) => res.json({ status: 'ok' }));\n\napp.listen(3000, () => console.log('Running on :3000'));\n".into()),
                    ("package.json".into(), "{\n  \"name\": \"api\",\n  \"version\": \"1.0.0\",\n  \"dependencies\": { \"express\": \"^4\" }\n}\n".into()),
                ]),
                post_commands: vec!["node --check index.js".into()],
            },
            Template {
                name: "react-vite".into(),
                description: "React + Vite frontend".into(),
                stack: "javascript".into(),
                files: HashMap::from([
                    ("package.json".into(), "{\n  \"name\": \"frontend\",\n  \"scripts\": { \"dev\": \"vite\" },\n  \"dependencies\": { \"react\": \"^18\", \"react-dom\": \"^18\" }\n}\n".into()),
                    ("src/App.jsx".into(), "export default function App() {\n  return <h1>Hello from Hive</h1>;\n}\n".into()),
                ]),
                post_commands: vec![],
            },
            Template {
                name: "go-chi-api".into(),
                description: "Go Chi REST API".into(),
                stack: "go".into(),
                files: HashMap::from([
                    ("main.go".into(), "package main\n\nimport (\n\t\"fmt\"\n\t\"net/http\"\n)\n\nfunc main() {\n\thttp.HandleFunc(\"/health\", func(w http.ResponseWriter, r *http.Request) {\n\t\tfmt.Fprint(w, \"ok\")\n\t})\n\thttp.ListenAndServe(\":8080\", nil)\n}\n".into()),
                    ("go.mod".into(), "module api\n\ngo 1.21\n".into()),
                ]),
                post_commands: vec!["go build .".into()],
            },
            Template {
                name: "dart-server".into(),
                description: "Dart shelf server".into(),
                stack: "dart".into(),
                files: HashMap::from([
                    ("pubspec.yaml".into(), "name: api\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\ndependencies:\n  shelf: ^1.4.0\n  shelf_router: ^1.1.0\n".into()),
                    ("bin/server.dart".into(), "import 'package:shelf/shelf.dart';\nimport 'package:shelf/shelf_io.dart' as io;\n\nvoid main() async {\n  var handler = const Pipeline().addMiddleware(logRequests()).addHandler((req) => Response.ok('ok'));\n  var server = await io.serve(handler, 'localhost', 8080);\n  print('Serving at http://localhost:8080');\n}\n".into()),
                ]),
                post_commands: vec!["dart analyze".into()],
            },
        ]
    }

    pub fn get(&self, name: &str) -> Option<&Template> {
        self.templates.iter().find(|t| t.name == name)
    }

    pub fn list(&self) -> &[Template] {
        &self.templates
    }

    pub fn list_by_stack(&self, stack: &str) -> Vec<&Template> {
        self.templates.iter().filter(|t| t.stack == stack).collect()
    }

    /// Aplica un template a un directorio.
    pub fn apply(&self, name: &str, target: &Path) -> Result<Vec<String>, String> {
        let template = self
            .get(name)
            .ok_or(format!("template '{name}' no encontrado"))?;
        let mut created = Vec::new();

        for (file_path, content) in &template.files {
            let full_path = target.join(file_path);
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&full_path, content).map_err(|e| e.to_string())?;
            created.push(file_path.clone());
        }

        Ok(created)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_templates() {
        let reg = TemplateRegistry::new();
        assert!(reg.list().len() >= 6);
    }

    #[test]
    fn get_by_name() {
        let reg = TemplateRegistry::new();
        assert!(reg.get("rust-actix-api").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn list_by_stack() {
        let reg = TemplateRegistry::new();
        assert!(!reg.list_by_stack("rust").is_empty());
    }

    #[test]
    fn apply_creates_files() {
        let tmp = tempfile::tempdir().unwrap();
        let reg = TemplateRegistry::new();
        let created = reg.apply("python-fastapi", tmp.path()).unwrap();
        assert!(created.contains(&"main.py".to_string()));
        assert!(tmp.path().join("main.py").exists());
    }
}
