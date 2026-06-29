use crate::schema::Metadata;

use ndarray::Array2;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;

/// Common HTML tags for one‑hot encoding (order defines index)
const TAG_LIST: &[&str] = &[
    "div", "p", "span", "a", "img", "button", "input", "li", "ul", "ol", "h1", "h2", "h3", "h4",
    "h5", "h6", "section", "article", "nav", "header", "footer",
];

fn tag_to_one_hot(tag: &str) -> [f32; 20] {
    let mut one_hot = [0.0; 20];
    if let Some(idx) = TAG_LIST.iter().position(|&t| t == tag) {
        one_hot[idx] = 1.0;
    }
    one_hot
}

/// Converts DOM nodes to a fixed‑width inference tensor.
///
/// # Arguments
/// * `metadata` - Root of the FlatBuffer containing nodes and viewport dimensions.
/// * `max_nodes` - Maximum number of nodes expected by the model.
/// * `feature_dim` - Feature dimension expected by the model (default 410, may be truncated/padded).
///
/// # Returns
/// An `Arc<[f32]>` of length `max_nodes * feature_dim` in row‑major order.
pub fn dom_to_tensor(metadata: &Metadata, max_nodes: usize, feature_dim: usize) -> Arc<[f32]> {
    let viewport_width = metadata.width() as f32;
    let viewport_height = metadata.height() as f32;
    let viewport_area = viewport_width * viewport_height;

    // Retrieve the node vector; if missing, return zero tensor immediately.
    let nodes_vec = match metadata.nodes() {
        Some(v) => v,
        None => return Arc::from(vec![0.0f32; max_nodes * feature_dim].into_boxed_slice()),
    };
    let node_count = nodes_vec.len();

    let mut tensor = vec![0.0f32; max_nodes * feature_dim];

    for i in 0..max_nodes {
        // Get node if within bounds, else sentinel (None)
        let node_opt = if i < node_count {
            Some(nodes_vec.get(i))
        } else {
            None
        };

        let features = if let Some(n) = node_opt {
            let mut feats = Vec::with_capacity(410);

            // 1. Tag one‑hot (20)
            let tag = n.tag().unwrap_or("");
            feats.extend_from_slice(&tag_to_one_hot(tag));

            // 2. Text presence (1)
            feats.push(if n.has_text() { 1.0 } else { 0.0 });

            // 3. Text embedding (384) – placeholder zeros
            feats.extend(std::iter::repeat(0.0).take(384));

            // 4. Bounding rect (4) normalized
            let rect = n.rect();
            if let Some(r) = rect {
                feats.push(r.x() / viewport_width);
                feats.push(r.y() / viewport_height);
                feats.push(r.width() / viewport_width);
                feats.push(r.height() / viewport_height);
                // 5. Area (1) normalized
                let area = r.width() * r.height();
                feats.push(area / viewport_area);
            } else {
                feats.extend_from_slice(&[0.0, 0.0, 0.0, 0.0, 0.0]);
            }

            feats
        } else {
            // sentinel: all zeros, length 410
            vec![0.0; 410]
        };

        // Truncate or pad to feature_dim
        let start = i * feature_dim;
        let end = start + feature_dim.min(features.len());
        tensor[start..end].copy_from_slice(&features[..end - start]);
        // any remaining positions (if feature_dim > features.len()) are already zero
    }

    Arc::from(tensor.into_boxed_slice())
}
