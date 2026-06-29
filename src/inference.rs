use ort::session::Session;
use ort::value::Value;
use std::error::Error;
use std::time::Instant;
// Ensure VisualAction is defined in crate::protocol
use crate::protocol::VisualAction;

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
    let outputs = session.run(ort::inputs!["input" => input_value])?;
    let (_shape, data) = outputs[0].try_extract_tensor::<f32>()?;
    let duration = start.elapsed();
    println!(
        "Inference completed in {:?}, output[0]={:?}",
        duration,
        data.first()
    );
    // TODO: Convert output tensor to actions using per-node thresholds and node rects.
    // Requires node rects to be passed into this function (not only tensor).
    // For now, return empty list.
    Ok(Vec::new())
}
