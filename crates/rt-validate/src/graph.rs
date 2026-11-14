//! Pass 8: reference cycles (Tarjan) and dependency metadata (design §3.6.1, Req 6.9, 6.10).
//!
//! Everything here is iterative, so a long reference chain cannot overflow the stack.

use std::collections::VecDeque;

const UNSEEN: usize = usize::MAX;

fn get(v: &[usize], i: usize) -> usize {
    v.get(i).copied().unwrap_or(UNSEEN)
}

fn put(v: &mut [usize], i: usize, x: usize) {
    if let Some(slot) = v.get_mut(i) {
        *slot = x;
    }
}

/// Strongly connected components of the graph `adj` (node `i` has edges to `adj[i]`), in reverse
/// topological order: a component is listed after every component it points to. Members of one
/// component are in ascending node order.
pub(crate) fn tarjan(adj: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let n = adj.len();
    let mut index = vec![UNSEEN; n];
    let mut low = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut comps: Vec<Vec<usize>> = Vec::new();
    let mut next = 0usize;
    for root in 0..n {
        if get(&index, root) != UNSEEN {
            continue;
        }
        let mut calls: Vec<(usize, usize)> = vec![(root, 0)];
        put(&mut index, root, next);
        put(&mut low, root, next);
        next += 1;
        stack.push(root);
        if let Some(f) = on_stack.get_mut(root) {
            *f = true;
        }
        while let Some(&(v, i)) = calls.last() {
            let succ = adj.get(v).and_then(|a| a.get(i)).copied();
            if let Some(w) = succ {
                if let Some(top) = calls.last_mut() {
                    top.1 += 1;
                }
                if get(&index, w) == UNSEEN {
                    put(&mut index, w, next);
                    put(&mut low, w, next);
                    next += 1;
                    stack.push(w);
                    if let Some(f) = on_stack.get_mut(w) {
                        *f = true;
                    }
                    calls.push((w, 0));
                } else if on_stack.get(w).copied().unwrap_or(false) {
                    let m = get(&low, v).min(get(&index, w));
                    put(&mut low, v, m);
                }
            } else {
                calls.pop();
                if get(&low, v) == get(&index, v) {
                    let mut comp = Vec::new();
                    while let Some(w) = stack.pop() {
                        if let Some(f) = on_stack.get_mut(w) {
                            *f = false;
                        }
                        comp.push(w);
                        if w == v {
                            break;
                        }
                    }
                    comp.sort_unstable();
                    comps.push(comp);
                }
                if let Some(&(p, _)) = calls.last() {
                    let m = get(&low, p).min(get(&low, v));
                    put(&mut low, p, m);
                }
            }
        }
    }
    comps
}

/// True when the component is a cycle: it has several nodes, or one node with an edge to itself.
pub(crate) fn is_cycle(comp: &[usize], adj: &[Vec<usize>]) -> bool {
    match comp {
        [] => false,
        [only] => adj.get(*only).is_some_and(|a| a.contains(only)),
        _ => true,
    }
}

/// The cycle through the lowest node of `comp`, in the order of reference, starting at that
/// node: the shortest way back to it, breaking ties by ascending node order.
pub(crate) fn cycle_path(comp: &[usize], adj: &[Vec<usize>]) -> Vec<usize> {
    let Some(&start) = comp.first() else {
        return Vec::new();
    };
    let n = adj.len();
    let mut parent = vec![UNSEEN; n];
    let mut queue: VecDeque<usize> = VecDeque::new();
    queue.push_back(start);
    let mut seen = vec![false; n];
    if let Some(f) = seen.get_mut(start) {
        *f = true;
    }
    while let Some(u) = queue.pop_front() {
        let edges = adj.get(u).map_or(&[][..], Vec::as_slice);
        for &w in edges {
            if !comp.contains(&w) {
                continue;
            }
            if w == start {
                let mut path = vec![u];
                let mut cur = u;
                while cur != start {
                    cur = get(&parent, cur);
                    if cur == UNSEEN {
                        return vec![start];
                    }
                    path.push(cur);
                }
                path.reverse();
                return path;
            }
            if !seen.get(w).copied().unwrap_or(true) {
                if let Some(f) = seen.get_mut(w) {
                    *f = true;
                }
                put(&mut parent, w, u);
                queue.push_back(w);
            }
        }
    }
    vec![start]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_cycle_and_orders_components() {
        // 0 -> 1 -> 2 -> 0, 3 -> 0, 4 -> 4
        let adj = vec![vec![1], vec![2], vec![0], vec![0], vec![4]];
        let comps = tarjan(&adj);
        assert!(comps.contains(&vec![0, 1, 2]));
        assert!(comps.contains(&vec![3]));
        let pos = |c: &[usize]| comps.iter().position(|x| x == c).unwrap();
        assert!(pos(&[0, 1, 2]) < pos(&[3]));
        let cyc: Vec<_> = comps.iter().filter(|c| is_cycle(c, &adj)).collect();
        assert_eq!(cyc.len(), 2);
        assert_eq!(cycle_path(&[0, 1, 2], &adj), vec![0, 1, 2]);
        assert_eq!(cycle_path(&[4], &adj), vec![4]);
    }

    #[test]
    fn long_chain_does_not_overflow() {
        let n = 50_000;
        let adj: Vec<Vec<usize>> = (0..n)
            .map(|i| if i + 1 < n { vec![i + 1] } else { vec![] })
            .collect();
        assert_eq!(tarjan(&adj).len(), n);
    }
}
