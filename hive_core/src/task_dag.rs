//! DAG (Directed Acyclic Graph) de tareas con ejecución paralela.
//!
//! Permite modelar dependencias entre tareas y ejecutar en paralelo las independientes.
//! Inspirado en LangGraph que usa grafos dirigidos para orquestación de agentes.

#[cfg(feature = "multiagent")]
use anyhow::{Context, Result};
#[cfg(feature = "multiagent")]
use petgraph::algo::toposort;
#[cfg(feature = "multiagent")]
use petgraph::graph::{DiGraph, NodeIndex};
#[cfg(feature = "multiagent")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "multiagent")]
use std::collections::HashMap;
#[cfg(feature = "multiagent")]
use std::future::Future;
#[cfg(feature = "multiagent")]
use tracing::info;
#[cfg(feature = "multiagent")]
use uuid::Uuid;

/// Nodo en el DAG de tareas.
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNode {
    pub id: Uuid,
    pub name: String,
    pub specialist_key: String,
    pub priority: u32,
    pub timeout_secs: u64,
    pub completed: bool,
}

/// Resultado de ejecución de un nodo.
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResult {
    pub node_id: Uuid,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub duration_ms: u64,
}

/// Grafo dirigido de tareas con soporte de ejecución paralela por niveles.
#[cfg(feature = "multiagent")]
pub struct TaskDag {
    graph: DiGraph<TaskNode, ()>,
    node_map: HashMap<Uuid, NodeIndex>,
}

#[cfg(feature = "multiagent")]
impl TaskDag {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_map: HashMap::new(),
        }
    }

    /// Añade una tarea al grafo.
    pub fn add_task(&mut self, task: TaskNode) -> NodeIndex {
        let idx = self.graph.add_node(task.clone());
        self.node_map.insert(task.id, idx);
        idx
    }

    /// Añade una dependencia: `dependent` depende de `dependency`.
    pub fn add_dependency(&mut self, dependency: Uuid, dependent: Uuid) -> Result<()> {
        let dep_idx = self
            .node_map
            .get(&dependency)
            .context("nodo dependencia no encontrado")?;
        let dep_node = self
            .node_map
            .get(&dependent)
            .context("nodo dependiente no encontrado")?;
        self.graph.add_edge(*dep_idx, *dep_node, ());
        Ok(())
    }

    /// Obtiene los niveles de ejecución (grupos de tareas sin dependencias entre sí).
    pub fn execution_levels(&self) -> Result<Vec<Vec<Uuid>>> {
        let sorted = toposort(&self.graph, None)
            .map_err(|_| anyhow::anyhow!("ciclo detectado en el DAG"))?;

        // Calcular niveles: cada nodo va en el nivel = max(nivel de predecesores) + 1
        let mut levels: HashMap<NodeIndex, usize> = HashMap::new();
        for &node in &sorted {
            let parent_max = self
                .graph
                .neighbors_directed(node, petgraph::Direction::Incoming)
                .filter_map(|p| levels.get(&p).copied())
                .max();
            let my_level = parent_max.map(|m| m + 1).unwrap_or(0);
            levels.insert(node, my_level);
        }

        let max_level = levels.values().max().copied().unwrap_or(0);
        let mut result: Vec<Vec<Uuid>> = vec![vec![]; max_level + 1];

        for (node, &level) in &levels {
            if let Some(task) = self.graph.node_weight(*node) {
                result[level].push(task.id);
            }
        }

        Ok(result)
    }

    /// Número de nodos.
    pub fn len(&self) -> usize {
        self.graph.node_count()
    }

    pub fn is_empty(&self) -> bool {
        self.graph.node_count() == 0
    }
}

#[cfg(feature = "multiagent")]
impl Default for TaskDag {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "multiagent")]
impl TaskDag {
    /// Obtiene un nodo por UUID.
    pub fn get_task(&self, id: &Uuid) -> Option<&TaskNode> {
        self.node_map
            .get(id)
            .and_then(|idx| self.graph.node_weight(*idx))
    }

    /// Marca un nodo como completado.
    pub fn mark_completed(&mut self, id: &Uuid) {
        if let Some(idx) = self.node_map.get(id) {
            if let Some(node) = self.graph.node_weight_mut(*idx) {
                node.completed = true;
            }
        }
    }

    /// Verifica si todos los nodos están completados.
    pub fn all_completed(&self) -> bool {
        self.graph.node_weights().all(|n| n.completed)
    }
}

/// Ejecutor paralelo de niveles del DAG.
#[cfg(feature = "multiagent")]
pub struct DagExecutor;

#[cfg(feature = "multiagent")]
impl DagExecutor {
    /// Ejecuta el DAG nivel por nivel, en paralelo dentro de cada nivel.
    pub async fn execute<F, Fut>(dag: &mut TaskDag, executor: F) -> Result<Vec<NodeResult>>
    where
        F: Fn(Uuid) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = NodeResult> + Send,
    {
        let levels = dag.execution_levels()?;
        let mut all_results = Vec::new();

        for (level_idx, level) in levels.iter().enumerate() {
            info!(
                level = level_idx,
                tasks = level.len(),
                "ejecutando nivel del DAG"
            );

            let mut handles = Vec::new();
            for &task_id in level {
                let exec = executor.clone();
                handles.push(tokio::spawn(async move { exec(task_id).await }));
            }

            for handle in handles {
                let result = handle.await.context("join task execution")?;
                if result.success {
                    dag.mark_completed(&result.node_id);
                }
                all_results.push(result);
            }
        }

        Ok(all_results)
    }
}

// Stubs sin la feature
#[cfg(not(feature = "multiagent"))]
pub struct TaskDag;

#[cfg(not(feature = "multiagent"))]
impl TaskDag {
    pub fn new() -> Self {
        Self
    }
    pub fn len(&self) -> usize {
        0
    }
    pub fn is_empty(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "multiagent")]
    #[test]
    fn dag_basico_con_dependencias() {
        let mut dag = TaskDag::new();
        let t1 = TaskNode {
            id: Uuid::new_v4(),
            name: "Análisis".into(),
            specialist_key: "rust".into(),
            priority: 1,
            timeout_secs: 60,
            completed: false,
        };
        let t2 = TaskNode {
            id: Uuid::new_v4(),
            name: "Obrera Rust".into(),
            specialist_key: "rust".into(),
            priority: 2,
            timeout_secs: 120,
            completed: false,
        };
        let t3 = TaskNode {
            id: Uuid::new_v4(),
            name: "Tests".into(),
            specialist_key: "test".into(),
            priority: 2,
            timeout_secs: 60,
            completed: false,
        };

        let id1 = t1.id;
        let id2 = t2.id;
        let id3 = t3.id;

        dag.add_task(t1);
        dag.add_task(t2);
        dag.add_task(t3);

        dag.add_dependency(id1, id2).unwrap();
        dag.add_dependency(id1, id3).unwrap();

        let levels = dag.execution_levels().unwrap();
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[0].len(), 1); // Análisis solo
        assert_eq!(levels[1].len(), 2); // Obrera y Tests en paralelo
    }

    #[cfg(feature = "multiagent")]
    #[test]
    fn dag_sin_dependencias_todo_en_paralelo() {
        let mut dag = TaskDag::new();
        let t1 = TaskNode {
            id: Uuid::new_v4(),
            name: "A".into(),
            specialist_key: "a".into(),
            priority: 1,
            timeout_secs: 60,
            completed: false,
        };
        let t2 = TaskNode {
            id: Uuid::new_v4(),
            name: "B".into(),
            specialist_key: "b".into(),
            priority: 1,
            timeout_secs: 60,
            completed: false,
        };
        dag.add_task(t1);
        dag.add_task(t2);

        let levels = dag.execution_levels().unwrap();
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0].len(), 2);
    }
}
