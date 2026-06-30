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
) -> Result<Vec<VisualAction>, Box<dyn Error + Send + Sync>> {
    let start = Instant::now();

    // Collect all input names, shapes, and types into a standard vector
    let mut input_values = Vec::new();

    // We store the session inputs metadata locally first to avoid borrowing conflicts
    let inputs = session.inputs();
    if inputs.is_empty() {
        return Err("Model has no inputs".into());
    }

    for input_info in inputs.iter() {
        let name = input_info.name().to_string();

        let (shape, element_type) = match input_info.dtype() {
            ValueType::Tensor { shape, ty, .. } => (shape, ty),
            _ => return Err("Expected a tensor input type".into()),
        };

        // Resolve dynamic dimensions
        let concrete_shape: Vec<usize> = shape
            .iter()
            .map(|&d| {
                if d <= 0 {
                    if name.contains("mask") || name.contains("id") {
                        128 // default sequence length for text tokens
                    } else {
                        1 // default batch size
                    }
                } else {
                    d as usize
                }
            })
            .collect();

        // Create zero-filled tensor
        let value: DynValue = match element_type {
            TensorElementType::Float32 => {
                let size = concrete_shape.iter().product();
                let data = vec![0.0f32; size];
                let array = ndarray::ArrayD::from_shape_vec(concrete_shape, data)?;
                Value::from_array(array)?.into_dyn()
            }
            TensorElementType::Int64 => {
                let size = concrete_shape.iter().product();
                let data = vec![0i64; size];
                let array = ndarray::ArrayD::from_shape_vec(concrete_shape, data)?;
                Value::from_array(array)?.into_dyn()
            }
            _ => {
                return Err(format!("Unsupported tensor type: {:?}", element_type).into());
            }
        };

        input_values.push((name, value));
    }

    // FIX: Pass the dynamic vector directly to session.run().
    // In ort v2, session.run() natively accepts any type implementing IntoIterator for (InterfaceName, Value).
    let outputs = session.run(input_values)?;

    let duration = start.elapsed();
    println!(
        "Large model inference completed successfully in {:?}. Output tensors: {}",
        duration,
        outputs.len()
    );

    Ok(Vec::new())
}
