//! Tipos compartidos entre orquestador y agentes (evita dependencia circular).

use crate::discovery::SpecialistProfile;
use crate::request::HiveWorkMode;
use uuid::Uuid;

/// Política de rama obrera: **derivada** de `main` o **huérfana** (raíz sin padre en la rama obrera).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerBranchMode {
    DerivedFromMain,
    /// Primera revisión en la rama es un commit sin padres; fusión a `main` con ancestro vacío.
    OrphanRoot,
}

impl Default for WorkerBranchMode {
    fn default() -> Self {
        WorkerBranchMode::DerivedFromMain
    }
}

#[derive(Debug, Clone)]
pub struct WorkerTask {
    pub id: Uuid,
    pub title: String,
    pub specialist: SpecialistProfile,
    pub branch_mode: WorkerBranchMode,
    /// Resumen corto para el título (puede estar vacío en mejora sin solicitud).
    pub mission_one_liner: String,
    /// Texto markdown para MR / Consejo (objetivo o mejora continua).
    pub mission_brief: String,
    /// `build` o `improve` (nunca `auto` aquí: ya resuelto en La Reina).
    pub work_mode: HiveWorkMode,
}

/// Estado de una tarea obrera.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    UnderReview,
    Completed,
    Failed,
}
