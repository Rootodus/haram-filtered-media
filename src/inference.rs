use crate::protocol::VisualAction;
use crate::types::DomNode;
use ort::session::Session;
// In v2.0.0-rc.12, these variants live directly inside the value submodule
use ort::value::DynValue;
use std::error::Error;
use std::time::Instant;

pub fn run_inference(
    session: &mut Session,
    input_ids: &DynValue,
    attention_mask: &DynValue,
    nodes: &[DomNode],
    viewport_width: f32,
    viewport_height: f32,
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
        "Large model inference completed in {:?}. Logits: neg={:.3}, pos={:.3}",
        duration, neg, pos
    );

    let mut actions = Vec::new();

    if neg > pos {
        for node in nodes {
            actions.push(VisualAction {
                action_type: 0, // BLUR
                rect: [node.rect.x, node.rect.y, node.rect.width, node.rect.height],
            });
        }
        println!(
            "Blur applied to {} nodes (negative sentiment)",
            actions.len()
        );
    } else {
        println!("Positive sentiment – no actions applied");
    }

    Ok(actions)
}
