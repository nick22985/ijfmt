use super::{Block, HashMap, HashSet, Wrap, WrapId};

pub(super) fn is_child_of(
    wid: WrapId,
    parent_wid: WrapId,
    wraps: &HashMap<WrapId, Wrap>,
    extra_parents: &HashMap<WrapId, HashSet<WrapId>>,
    relation_ignores: &HashSet<(WrapId, WrapId, usize)>,
    leaf_idx: usize,
) -> bool {
    if relation_ignores.contains(&(wid, parent_wid, leaf_idx)) {
        return false;
    }

    let wrap = match wraps.get(&wid) {
        Some(w) => w,
        None => return false,
    };

    if wrap.ignore_parent_wraps {
        return false;
    }

    let mut seen: HashSet<WrapId> = HashSet::new();
    let mut queue: Vec<WrapId> = Vec::new();
    let push_parents = |w: WrapId, q: &mut Vec<WrapId>| {
        if let Some(p) = wraps.get(&w).and_then(|x| x.parent) {
            q.push(p);
        }
        if let Some(ps) = extra_parents.get(&w) {
            q.extend(ps.iter().copied());
        }
    };
    push_parents(wid, &mut queue);
    let mut steps = 0;
    while let Some(pid) = queue.pop() {
        if pid == parent_wid {
            return true;
        }
        steps += 1;
        if steps > 512 {
            break;
        }
        if !seen.insert(pid) {
            continue;
        }
        if wraps.get(&pid).is_some_and(|w| w.ignore_parent_wraps) {
            continue;
        }
        push_parents(pid, &mut queue);
    }

    false
}

pub fn collect_wrap_parents(root: &Block) -> HashMap<WrapId, HashSet<WrapId>> {
    fn reaches(from: WrapId, target: WrapId, map: &HashMap<WrapId, HashSet<WrapId>>) -> bool {
        let mut seen: HashSet<WrapId> = HashSet::new();
        let mut queue = vec![from];
        while let Some(w) = queue.pop() {
            if w == target {
                return true;
            }
            if !seen.insert(w) {
                continue;
            }
            if let Some(ps) = map.get(&w) {
                queue.extend(ps.iter().copied());
            }
        }
        false
    }

    fn rec(b: &Block, cur: Option<WrapId>, out: &mut HashMap<WrapId, HashSet<WrapId>>) {
        let mut cur = cur;
        if let Some(w) = &b.wrap {
            if let Some(p) = cur
                && p != w.id
                && !reaches(p, w.id, out)
            {
                out.entry(w.id).or_default().insert(p);
            }
            if !b.wrap_no_promote {
                cur = Some(w.id);
            }
        }
        for c in &b.children {
            rec(c, cur, out);
        }
    }

    let mut out = HashMap::new();
    rec(root, None, &mut out);
    out
}
