//! Dashboard layout tree mutation helpers.

#![allow(unused_imports)]

use crate::app::SaveDashboardLayout;
use crate::contracts::{BoardNode, DashboardLayout};
use leptos::prelude::*;

pub(crate) fn collect_placed_kinds(layout: &DashboardLayout) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    fn walk(nodes: &[BoardNode], set: &mut std::collections::HashSet<String>) {
        for node in nodes {
            match node {
                BoardNode::Widget { kind, .. } => {
                    set.insert(kind.as_str().to_owned());
                }
                BoardNode::Container { children, .. } => walk(children, set),
            }
        }
    }
    walk(&layout.nodes, &mut set);
    set
}

pub(crate) fn next_node_id(prefix: &str, layout: &DashboardLayout) -> String {
    let mut n = layout.total_nodes().saturating_add(1);
    loop {
        let candidate = format!("{prefix}-{n}");
        if !id_exists(&layout.nodes, &candidate) {
            return candidate;
        }
        n += 1;
    }
}

pub(crate) fn id_exists(nodes: &[BoardNode], id: &str) -> bool {
    for node in nodes {
        if node.id() == id {
            return true;
        }
        if let BoardNode::Container { children, .. } = node
            && id_exists(children, id)
        {
            return true;
        }
    }
    false
}

pub(crate) fn remove_node(nodes: &mut Vec<BoardNode>, id: &str) -> bool {
    if let Some(idx) = nodes.iter().position(|n| n.id() == id) {
        nodes.remove(idx);
        return true;
    }
    for node in nodes.iter_mut() {
        if let BoardNode::Container { children, .. } = node
            && remove_node(children, id)
        {
            return true;
        }
    }
    false
}

/// Reorder two nodes that share the same parent list (root or container children).
pub(crate) fn reorder_siblings(nodes: &mut Vec<BoardNode>, from_id: &str, to_id: &str) -> bool {
    if from_id == to_id {
        return false;
    }
    if let (Some(from_idx), Some(to_idx)) = (
        nodes.iter().position(|n| n.id() == from_id),
        nodes.iter().position(|n| n.id() == to_id),
    ) {
        let mut to_idx = to_idx;
        let item = nodes.remove(from_idx);
        if from_idx < to_idx {
            to_idx -= 1;
        }
        nodes.insert(to_idx.min(nodes.len()), item);
        return true;
    }
    for node in nodes.iter_mut() {
        if let BoardNode::Container { children, .. } = node
            && reorder_siblings(children, from_id, to_id)
        {
            return true;
        }
    }
    false
}

pub(crate) fn set_span_by_id(nodes: &mut [BoardNode], id: &str, span: u8) -> bool {
    for node in nodes.iter_mut() {
        if node.id() == id {
            node.set_col_span(span);
            return true;
        }
        if let BoardNode::Container { children, .. } = node
            && set_span_by_id(children, id, span)
        {
            return true;
        }
    }
    false
}

pub(crate) fn find_col_span(nodes: &[BoardNode], id: &str) -> Option<u8> {
    for node in nodes {
        if node.id() == id {
            return Some(node.col_span());
        }
        if let BoardNode::Container { children, .. } = node
            && let Some(span) = find_col_span(children, id)
        {
            return Some(span);
        }
    }
    None
}

pub(crate) fn find_node<'a>(nodes: &'a [BoardNode], id: &str) -> Option<&'a BoardNode> {
    for node in nodes {
        if node.id() == id {
            return Some(node);
        }
        if let BoardNode::Container { children, .. } = node
            && let Some(found) = find_node(children, id)
        {
            return Some(found);
        }
    }
    None
}

/// Remove a node from the tree and return it.
pub(crate) fn extract_node(nodes: &mut Vec<BoardNode>, id: &str) -> Option<BoardNode> {
    if let Some(idx) = nodes.iter().position(|n| n.id() == id) {
        return Some(nodes.remove(idx));
    }
    for node in nodes.iter_mut() {
        if let BoardNode::Container { children, .. } = node
            && let Some(found) = extract_node(children, id)
        {
            return Some(found);
        }
    }
    None
}

/// Append `child` under the container with `parent_id` (any depth).
pub(crate) fn append_child(nodes: &mut Vec<BoardNode>, parent_id: &str, child: BoardNode) -> bool {
    fn walk(nodes: &mut Vec<BoardNode>, parent_id: &str, child: &mut Option<BoardNode>) -> bool {
        for node in nodes.iter_mut() {
            if let BoardNode::Container { id, children, .. } = node {
                if id == parent_id {
                    if let Some(c) = child.take() {
                        children.push(c);
                        return true;
                    }
                    return false;
                }
                if walk(children, parent_id, child) {
                    return true;
                }
            }
        }
        false
    }
    let mut child = Some(child);
    walk(nodes, parent_id, &mut child)
}

/// True if `ancestor_id` is `node_id` or contains it.
pub(crate) fn is_ancestor_of(nodes: &[BoardNode], ancestor_id: &str, node_id: &str) -> bool {
    if ancestor_id == node_id {
        return true;
    }
    let Some(BoardNode::Container { children, .. }) = find_node(nodes, ancestor_id) else {
        return false;
    };
    id_exists(children, node_id)
}

/// Move an existing node into a container (appended to its children).
pub(crate) fn move_into_container(
    nodes: &mut Vec<BoardNode>,
    node_id: &str,
    container_id: &str,
) -> bool {
    if node_id == container_id {
        return false;
    }
    // Target must be a container.
    if !matches!(
        find_node(nodes, container_id),
        Some(BoardNode::Container { .. })
    ) {
        return false;
    }
    // Refuse nesting a container inside itself / its descendants.
    if is_ancestor_of(nodes, node_id, container_id) {
        return false;
    }
    let Some(node) = extract_node(nodes, node_id) else {
        return false;
    };
    append_child(nodes, container_id, node)
}

/// Insert a new node either at board root or inside a target container.
pub(crate) fn place_new_node(
    nodes: &mut Vec<BoardNode>,
    child: BoardNode,
    parent_id: Option<&str>,
) -> bool {
    match parent_id {
        None | Some("") => {
            nodes.push(child);
            true
        }
        Some(parent) => append_child(nodes, parent, child),
    }
}

pub(crate) fn commit_layout(
    layout: RwSignal<DashboardLayout>,
    save_layout: ServerAction<SaveDashboardLayout>,
    next: DashboardLayout,
) {
    // Write signal first so fine-grained attrs (data-span / chip active) update immediately.
    layout.set(next.clone());
    save_layout.dispatch(SaveDashboardLayout { layout: next });
}
