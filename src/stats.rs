use crate::color::Category;
use crate::model::Node;

pub struct ExtStat {
    pub category: Category,
    pub size: u64,
    pub count: u64,
}

/// Extension/category breakdown for `node`'s subtree. This is a direct read
/// of the totals precomputed bottom-up at scan time (see `scanner::scan_dir`)
/// — no re-walking the subtree, so it stays instant even for a directory
/// with millions of descendants.
pub fn extension_stats(node: &Node) -> Vec<ExtStat> {
    let mut v: Vec<ExtStat> = Category::ALL
        .iter()
        .enumerate()
        .filter_map(|(i, &category)| {
            let (size, count) = *node.ext_totals.get(i)?;
            if count > 0 {
                Some(ExtStat {
                    category,
                    size,
                    count,
                })
            } else {
                None
            }
        })
        .collect();
    v.sort_by(|a, b| b.size.cmp(&a.size));
    v
}
