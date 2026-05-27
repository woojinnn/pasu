//! DerivedFrom dependency 그래프의 위상정렬.
//!
//! DerivedFrom 은 다른 LiveField 들의 값을 input 으로 받는다 (예: aave_hf 는
//! collateral_value, debt_value, liq_threshold 에 의존). 따라서 sync 가 derived
//! 들을 처리할 때:
//!   1. input 으로 쓰이는 source-of-truth 필드 (OnchainView, OracleFeed 등) 먼저 갱신
//!   2. DerivedFrom 들끼리는 의존 관계 따라 위상정렬 후 차례로 계산
//!
//! Kahn's algorithm 으로 단순 구현. cycle 이 있으면 `SyncError::CyclicDeps`.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::SyncError;

/// 한 노드 — 노드 id 와 자기 의존성 (input) 들의 id 목록.
#[derive(Clone, Debug)]
pub struct DepNode {
    pub id: String,
    /// 이 노드의 input 들. 모두 처리된 후에야 이 노드 실행.
    pub depends_on: Vec<String>,
}

/// nodes → 처리 순서 (먼저 처리해야 할 것이 앞).
pub fn topological_sort(nodes: Vec<DepNode>) -> Result<Vec<String>, SyncError> {
    // adjacency: dep_id → 이 dep 가 풀리면 진행 가능해지는 노드들
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

    // dep 들의 indegree 는 0 (입력 없음).
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
            sorted
                .last()
                .cloned()
                .unwrap_or_else(|| "<unknown>".into()),
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
