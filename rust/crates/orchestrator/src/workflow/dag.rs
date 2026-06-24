use std::collections::{HashMap, HashSet};

use super::errors::{Result, WorkflowError};
use super::model::NodeRequest;

pub fn validate_dag(nodes: &[NodeRequest]) -> Result<Vec<Vec<String>>> {
    if nodes.is_empty() {
        return Err(WorkflowError::InvalidInput("workflow must have at least one node".to_string()));
    }

    let mut names = HashSet::with_capacity(nodes.len());
    for node in nodes {
        let Some(name) = node.name.as_deref() else {
            return Err(WorkflowError::InvalidInput("node name is required".to_string()));
        };
        if !names.insert(name.to_string()) {
            return Err(WorkflowError::InvalidInput(format!("duplicate node name: \"{name}\"")));
        }
    }

    let mut indegree: HashMap<String, usize> = HashMap::with_capacity(nodes.len());
    let mut dependents: HashMap<String, Vec<String>> = HashMap::with_capacity(nodes.len());
    for node in nodes {
        let name = node.name.as_ref().expect("validated name").clone();
        indegree.entry(name.clone()).or_insert(0);
        for dep in &node.depends_on {
            if !names.contains(dep) {
                return Err(WorkflowError::InvalidInput(format!(
                    "node \"{}\" depends on unknown node \"{}\"",
                    name, dep
                )));
            }
            *indegree.entry(name.clone()).or_insert(0) += 1;
            dependents.entry(dep.clone()).or_default().push(name.clone());
        }
    }

    let mut remaining = indegree.len();
    let mut layers = Vec::new();
    while remaining > 0 {
        let layer: Vec<String> =
            indegree.iter().filter_map(|(name, degree)| if *degree == 0 { Some(name.clone()) } else { None }).collect();
        if layer.is_empty() {
            return Err(WorkflowError::InvalidInput("cycle detected in workflow graph".to_string()));
        }
        for name in &layer {
            indegree.remove(name);
            if let Some(next) = dependents.get(name) {
                for dependent in next {
                    if let Some(value) = indegree.get_mut(dependent) {
                        *value -= 1;
                    }
                }
            }
        }
        remaining -= layer.len();
        layers.push(layer);
    }

    Ok(layers)
}
