//! Tree-shaped tag listing. Wraps the flat `queries::list_tags` and
//! builds an ordered tree from `parent_id`. Cycle-safe: a `HashSet`
//! guards the recursion so a malformed DB cannot loop the server.

use std::collections::{HashMap, HashSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::error::ThingsError;
use crate::core::reader::pool::ReaderPool;
use crate::core::reader::queries::list_tags;
use crate::core::types::Tag;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TagNode {
    pub id: String,
    pub title: String,
    pub children: Vec<TagNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TagListing {
    /// Every tag, ordered by Things' display index then title. Same shape
    /// `things_list_tags` returned in Plan 2.
    pub flat: Vec<Tag>,
    /// Tag trees rooted at top-level tags (those with no parent). Order
    /// matches `flat` (root tags appear in display-index order).
    pub roots: Vec<TagNode>,
}

pub async fn list_tags_with_tree(pool: &ReaderPool) -> Result<TagListing, ThingsError> {
    let flat = list_tags(pool).await?;
    let roots = build_tree(&flat);
    Ok(TagListing { flat, roots })
}

/// Build a tag tree from the flat list. Cycle-safe: each recursion path
/// maintains a `visited` set; a node that points back into the path is
/// dropped (with a `tracing::warn!`).
pub fn build_tree(flat: &[Tag]) -> Vec<TagNode> {
    // Group children by parent id; preserve flat order within each group.
    let mut children_by_parent: HashMap<&str, Vec<&Tag>> = HashMap::new();
    let mut roots: Vec<&Tag> = Vec::new();
    for tag in flat {
        match tag.parent_id.as_deref() {
            None => roots.push(tag),
            Some(pid) => children_by_parent.entry(pid).or_default().push(tag),
        }
    }

    let mut out = Vec::with_capacity(roots.len());
    for root in roots {
        let mut visited: HashSet<&str> = HashSet::new();
        visited.insert(root.id.as_str());
        out.push(build_node(root, &children_by_parent, &mut visited));
    }
    out
}

fn build_node<'a>(
    tag: &'a Tag,
    children_by_parent: &HashMap<&'a str, Vec<&'a Tag>>,
    visited: &mut HashSet<&'a str>,
) -> TagNode {
    let mut children = Vec::new();
    if let Some(child_list) = children_by_parent.get(tag.id.as_str()) {
        for child in child_list {
            if !visited.insert(child.id.as_str()) {
                tracing::warn!(
                    "tag cycle detected at uuid={}; dropping subtree",
                    child.id
                );
                continue;
            }
            children.push(build_node(child, children_by_parent, visited));
            visited.remove(child.id.as_str());
        }
    }
    TagNode {
        id: tag.id.clone(),
        title: tag.title.clone(),
        children,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reader::fixture::build_fixture;
    use tempfile::tempdir;

    #[tokio::test]
    async fn list_tags_with_tree_matches_fixture_two_level_nesting() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let listing = list_tags_with_tree(&pool).await.unwrap();
        // Flat: 3 tags total — Errand, Call, Deep work.
        assert_eq!(listing.flat.len(), 3);
        // Roots: 2 — Errand and Deep work (Call has parent Errand).
        assert_eq!(listing.roots.len(), 2);
        let titles: Vec<&str> = listing.roots.iter().map(|r| r.title.as_str()).collect();
        assert!(titles.contains(&"Errand"));
        assert!(titles.contains(&"Deep work"));
        // Errand has 1 child: Call.
        let errand = listing.roots.iter().find(|r| r.title == "Errand").unwrap();
        assert_eq!(errand.children.len(), 1);
        assert_eq!(errand.children[0].title, "Call");
        assert!(errand.children[0].children.is_empty());
        // Deep work has no children.
        let deep = listing.roots.iter().find(|r| r.title == "Deep work").unwrap();
        assert!(deep.children.is_empty());
    }

    #[test]
    fn build_tree_handles_multi_level_synthetic_nesting() {
        // a → b → c (3-level nesting) plus an unrelated root x.
        let flat = vec![
            Tag { id: "a".into(), title: "A".into(), parent_id: None,              shortcut: None },
            Tag { id: "b".into(), title: "B".into(), parent_id: Some("a".into()), shortcut: None },
            Tag { id: "c".into(), title: "C".into(), parent_id: Some("b".into()), shortcut: None },
            Tag { id: "x".into(), title: "X".into(), parent_id: None,              shortcut: None },
        ];
        let roots = build_tree(&flat);
        assert_eq!(roots.len(), 2);
        let a = roots.iter().find(|r| r.title == "A").unwrap();
        assert_eq!(a.children.len(), 1);
        assert_eq!(a.children[0].title, "B");
        assert_eq!(a.children[0].children.len(), 1);
        assert_eq!(a.children[0].children[0].title, "C");
    }

    #[test]
    fn build_tree_drops_cycle_without_looping() {
        // a → b and b → a (impossible in Things but possible in a corrupt
        // DB). build_tree must drop the cycle's back-edge, not infinite-loop.
        // Both a and b list a parent, so NEITHER is a root — build_tree
        // returns no roots. The cycle guard ensures we don't blow the stack
        // trying to walk it.
        let flat = vec![
            Tag { id: "a".into(), title: "A".into(), parent_id: Some("b".into()), shortcut: None },
            Tag { id: "b".into(), title: "B".into(), parent_id: Some("a".into()), shortcut: None },
        ];
        let roots = build_tree(&flat);
        assert!(roots.is_empty(), "no parentless tags -> no roots; cycle survived without crashing");
    }
}
