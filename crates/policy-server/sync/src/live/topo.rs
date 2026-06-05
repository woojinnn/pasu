use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::SyncError;

#[derive(Clone, Debug)]
pub struct DepNode {
    pub id: String,
    pub depends_on: Vec<String>,
}

pub fn topological_sort(nodes: Vec<DepNode>) -> Result<Vec<String>, SyncError> {
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    let mut all_ids: HashSet<String> = HashSet::new();

    for n in &nodes {
        all_ids.insert(n.id.clone());
        in_degree.entry(n.id.clone()).or_insert(0);
        for dep in &n.depends_on {
            all_ids.insert(dep.clone());
            adj.entry(dep.clone()).or_default().push(n.id.clone());
        }
    }

    for n in &nodes {
        let entry = in_degree.entry(n.id.clone()).or_insert(0);
        *entry = n.depends_on.len();
    }

    let mut queue: VecDeque<String> = VecDeque::new();
    for id in &all_ids {
        let d = in_degree.get(id).copied().unwrap_or(0);
        if d == 0 {
            queue.push_back(id.clone());
        }
    }

    let mut sorted: Vec<String> = Vec::with_capacity(all_ids.len());
    while let Some(id) = queue.pop_front() {
        sorted.push(id.clone());
        if let Some(next) = adj.get(&id) {
            for nxt in next.clone() {
                if let Some(d) = in_degree.get_mut(&nxt) {
                    *d -= 1;
                    if *d == 0 {
                        queue.push_back(nxt);
                    }
                }
            }
        }
    }

    if sorted.len() != all_ids.len() {
        return Err(SyncError::CyclicDeps(
            sorted.last().cloned().unwrap_or_else(|| "<unknown>".into()),
        ));
    }
    Ok(sorted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_chain() {
        // A → B → C
        let nodes = vec![
            DepNode {
                id: "A".into(),
                depends_on: vec![],
            },
            DepNode {
                id: "B".into(),
                depends_on: vec!["A".into()],
            },
            DepNode {
                id: "C".into(),
                depends_on: vec!["B".into()],
            },
        ];
        let sorted = topological_sort(nodes).unwrap();
        let a = sorted.iter().position(|s| s == "A").unwrap();
        let b = sorted.iter().position(|s| s == "B").unwrap();
        let c = sorted.iter().position(|s| s == "C").unwrap();
        assert!(a < b && b < c);
    }

    #[test]
    fn diamond() {
        // A → B, A → C, B → D, C → D
        let nodes = vec![
            DepNode {
                id: "A".into(),
                depends_on: vec![],
            },
            DepNode {
                id: "B".into(),
                depends_on: vec!["A".into()],
            },
            DepNode {
                id: "C".into(),
                depends_on: vec!["A".into()],
            },
            DepNode {
                id: "D".into(),
                depends_on: vec!["B".into(), "C".into()],
            },
        ];
        let sorted = topological_sort(nodes).unwrap();
        let a = sorted.iter().position(|s| s == "A").unwrap();
        let b = sorted.iter().position(|s| s == "B").unwrap();
        let c = sorted.iter().position(|s| s == "C").unwrap();
        let d = sorted.iter().position(|s| s == "D").unwrap();
        assert!(a < b && a < c);
        assert!(b < d && c < d);
    }

    #[test]
    fn cycle_detected() {
        // A → B → A
        let nodes = vec![
            DepNode {
                id: "A".into(),
                depends_on: vec!["B".into()],
            },
            DepNode {
                id: "B".into(),
                depends_on: vec!["A".into()],
            },
        ];
        let err = topological_sort(nodes).unwrap_err();
        assert!(matches!(err, SyncError::CyclicDeps(_)));
    }
}
