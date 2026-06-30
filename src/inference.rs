use crate::protocol::VisualAction;
use ort::session::Session;
// In v2.0.0-rc.12, these variants live directly inside the value submodule
use ort::value::{DynValue, TensorElementType, Value, ValueType};
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
) -> Result<Vec<VisualAction>, Box<dyn Error + Send + Sync>> {
    let start = Instant::now();

    let outputs = session.run(ort::inputs![
        "input_ids" => input_ids,
        "attention_mask" => attention_mask
    ])?;

    let duration = start.elapsed();
    println!(
        "Large model inference completed in {:?}. Output tensors: {}",
        duration,
        outputs.len()
    );

    Ok(Vec::new())
}
