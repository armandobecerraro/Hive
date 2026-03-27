//! Undo/rollback granular de cambios.
//!
//! Permite deshacer cambios específicos en lugar de abandonar ramas completas.
//! Mantiene un stack de operaciones reversables.

use serde::{Deserialize, Serialize};

/// Operación reversible.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoableOperation {
    pub id: String,
    pub description: String,
    pub operation_type: OperationType,
    pub file_path: String,
    pub before_content: Option<String>,
    pub after_content: Option<String>,
    pub timestamp: i64,
}

/// Tipo de operación.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationType {
    FileCreate,
    FileModify,
    FileDelete,
    GitCommit,
    GitBranch,
}

/// Stack de operaciones undoables.
pub struct UndoStack {
    operations: Vec<UndoableOperation>,
    redo_stack: Vec<UndoableOperation>,
    max_operations: usize,
}

impl UndoStack {
    pub fn new(max_operations: usize) -> Self {
        Self {
            operations: Vec::new(),
            redo_stack: Vec::new(),
            max_operations,
        }
    }

    /// Registra una operación.
    pub fn push(&mut self, op: UndoableOperation) {
        self.operations.push(op);
        self.redo_stack.clear();
        if self.operations.len() > self.max_operations {
            self.operations.remove(0);
        }
    }

    /// Deshace la última operación.
    pub fn undo(&mut self) -> Option<&UndoableOperation> {
        let op = self.operations.pop()?;
        self.redo_stack.push(op);
        self.redo_stack.last()
    }

    /// Rehace la última operación deshecha.
    pub fn redo(&mut self) -> Option<&UndoableOperation> {
        let op = self.redo_stack.pop()?;
        self.operations.push(op);
        self.operations.last()
    }

    /// Aplica el undo: restaura el contenido anterior.
    pub fn apply_undo(&mut self, repo_root: &std::path::Path) -> Result<String, String> {
        let op = self.undo().ok_or("nada que deshacer")?;
        match op.operation_type {
            OperationType::FileCreate => {
                let path = repo_root.join(&op.file_path);
                std::fs::remove_file(&path).map_err(|e| e.to_string())?;
                Ok(format!("eliminado: {}", op.file_path))
            }
            OperationType::FileModify => {
                if let Some(ref before) = op.before_content {
                    let path = repo_root.join(&op.file_path);
                    std::fs::write(&path, before).map_err(|e| e.to_string())?;
                    Ok(format!("restaurado: {}", op.file_path))
                } else {
                    Err("no hay contenido anterior".into())
                }
            }
            OperationType::FileDelete => {
                if let Some(ref before) = op.before_content {
                    let path = repo_root.join(&op.file_path);
                    std::fs::write(&path, before).map_err(|e| e.to_string())?;
                    Ok(format!("recuperado: {}", op.file_path))
                } else {
                    Err("no hay contenido para recuperar".into())
                }
            }
            _ => Ok(format!("undo registrado para: {}", op.description)),
        }
    }

    /// Lista operaciones pendientes.
    pub fn pending(&self) -> &[UndoableOperation] {
        &self.operations
    }

    /// Cuenta operaciones pendientes.
    pub fn len(&self) -> usize {
        self.operations.len()
    }
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    /// Limpia el stack.
    pub fn clear(&mut self) {
        self.operations.clear();
        self.redo_stack.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_op(desc: &str) -> UndoableOperation {
        UndoableOperation {
            id: uuid::Uuid::new_v4().to_string(),
            description: desc.into(),
            operation_type: OperationType::FileCreate,
            file_path: "test.rs".into(),
            before_content: None,
            after_content: Some("content".into()),
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    #[test]
    fn push_and_undo() {
        let mut stack = UndoStack::new(100);
        stack.push(sample_op("test"));
        assert_eq!(stack.len(), 1);
        let undone = stack.undo();
        assert!(undone.is_some());
        assert!(stack.is_empty());
    }

    #[test]
    fn redo_after_undo() {
        let mut stack = UndoStack::new(100);
        stack.push(sample_op("test"));
        stack.undo();
        let redone = stack.redo();
        assert!(redone.is_some());
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn max_operations_limits() {
        let mut stack = UndoStack::new(2);
        stack.push(sample_op("a"));
        stack.push(sample_op("b"));
        stack.push(sample_op("c"));
        assert_eq!(stack.len(), 2);
    }
}
