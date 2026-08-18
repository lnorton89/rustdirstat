use crate::color;
use crate::model::Node;
use std::collections::HashMap;

pub struct ExtStat {
    pub category: String,
    pub size: u64,
    pub count: u64,
}

/// Aggregate size/count per extension category across every file in the
/// subtree rooted at `node` (recursing through all descendant directories).
pub fn extension_stats(node: &Node) -> Vec<ExtStat> {
    let mut map: HashMap<String, (u64, u64)> = HashMap::new();
    accumulate(node, &mut map);
    let mut v: Vec<ExtStat> = map
        .into_iter()
        .map(|(category, (size, count))| ExtStat {
            category,
            size,
            count,
        })
        .collect();
    v.sort_by(|a, b| b.size.cmp(&a.size));
    v
}

fn accumulate(node: &Node, map: &mut HashMap<String, (u64, u64)>) {
    if node.is_dir {
        for c in &node.children {
            accumulate(c, map);
        }
    } else {
        let cat = color::category_for_ext(node.extension()).to_string();
        let entry = map.entry(cat).or_insert((0, 0));
        entry.0 += node.size;
        entry.1 += 1;
    }
}
