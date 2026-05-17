use crate::state::FrameState;

use ort::session::Session;
use ort::value::Value;
use std::error::Error;
use std::time::Instant;

pub fn run_inference(session: &mut Session, _frame: &FrameState) -> Result<(), Box<dyn Error>> {
    let start = Instant::now();
    let max_nodes = 256;
    let feature_dim = 410;
    let expected_size = max_nodes * feature_dim;

    let dummy_input: Vec<f32> = vec![0.0; expected_size];
    let input_array = ndarray::Array2::from_shape_vec((max_nodes, feature_dim), dummy_input)?;

    // Convert to ONNX-compatible value
    let input_value = Value::from_array(input_array)?;

    // Run the model
    let outputs = session.run(ort::inputs!["input" => input_value])?;

    // The method now returns (&Shape, &[f32])
    let (_shape, data) = outputs[0].try_extract_tensor::<f32>()?;
    let duration = start.elapsed();

    // To get an ndarray::ArrayViewD if needed later:
    // let view = ndarray::ArrayViewD::from_shape(shape, data)?;

    println!(
        "Inference completed in {:?}, output[0]={:?}",
        duration,
        data.first()
    );
    Ok(())
}
