use crate::protocol::VisualAction;
use crate::schema::Metadata;
use ort::session::Session;
// In v2.0.0-rc.12, these variants live directly inside the value submodule
use ort::value::{DynValue, Value};
use std::error::Error;
use std::time::Instant;

pub fn run_inference(
    session: &mut Session,
    tensor: &[f32],
    shape: (usize, usize), // (max_nodes, feature_dim)
) -> Result<Vec<VisualAction>, Box<dyn Error + Send + Sync>> {
    let start = Instant::now();
    let (rows, cols) = shape;
    let expected_size = rows * cols;
    if tensor.len() != expected_size {
        return Err(format!(
            "Tensor size mismatch: expected {}, got {}",
            expected_size,
            tensor.len()
        )
        .into());
    }

    let input_array = ndarray::Array2::from_shape_vec((rows, cols), tensor.to_vec())?;
    let input_value = Value::from_array(input_array)?;

    // Execute session runner - macro returns raw collection directly in v2.x
    let outputs = session.run(ort::inputs!["input" => input_value])?;
    let (_shape, data) = outputs[0].try_extract_tensor::<f32>()?;
    let duration = start.elapsed();
    println!(
        "Inference completed in {:?}, output={:?}",
        duration,
        data.first()
    );

    Ok(Vec::new())
}

pub fn run_inference_large(
    session: &mut Session,
    input_ids: &DynValue,
    attention_mask: &DynValue,
    metadata: Option<&Metadata>, // now optional
) -> Result<Vec<VisualAction>, Box<dyn Error + Send + Sync>> {
    let start = Instant::now();

    let outputs = session.run(ort::inputs![
        "input_ids" => input_ids,
        "attention_mask" => attention_mask
    ])?;

    let (_shape, logits) = outputs[0].try_extract_tensor::<f32>()?;
    let neg = logits.get(0).copied().unwrap_or(0.0);
    let pos = logits.get(1).copied().unwrap_or(0.0);

    let duration = start.elapsed();
    println!(
        "Large model inference completed in {:?}. Output tensors: {}. Logits: neg={:.3}, pos={:.3}",
        duration,
        outputs.len(),
        neg,
        pos
    );

    let mut actions = Vec::new();

    // Only generate actions if metadata is provided (i.e., real inference)
    if let Some(meta) = metadata {
        if neg > pos {
            let nodes = meta.nodes().unwrap_or_default();
            for i in 0..nodes.len() {
                let node = nodes.get(i);
                if let Some(rect) = node.rect() {
                    actions.push(VisualAction {
                        action_type: 0,
                        rect: [rect.x(), rect.y(), rect.width(), rect.height()],
                    });
                }
            }
            println!(
                "Blur applied to {} nodes (negative sentiment)",
                actions.len()
            );
        } else {
            println!("Positive sentiment – no actions applied");
        }
    } else {
        println!("Warmup inference – no actions generated (metadata absent)");
    }

    Ok(actions)
}
