use crate::types::DomNode;
use chromiumoxide::Page;
use serde::Deserialize;
use std::error::Error;

/// Temporary struct for deserialising the JS response.
#[derive(Debug, Deserialize)]
struct JsNode {
    id: u32,
    tag: String,
    has_text: bool,
    text: Option<String>,
    rect: JsRect,
}

#[derive(Debug, Deserialize)]
struct JsRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl From<JsRect> for crate::types::Rect {
    fn from(r: JsRect) -> Self {
        crate::types::Rect {
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        }
    }
}

/// Extracts DOM nodes matching the given CSS selector.
pub async fn extract_dom_nodes(
    page: &Page,
    selector: &str,
) -> Result<Vec<DomNode>, Box<dyn Error>> {
    let js_code = format!(
        r#"
        (() => {{
            const elements = document.querySelectorAll('{}');
            return Array.from(elements).map((el, idx) => {{
                const rect = el.getBoundingClientRect();
                const text = el.textContent?.trim() ?? null;
                return {{
                    id: idx,
                    tag: el.tagName.toLowerCase(),
                    has_text: text !== null && text.length > 0,
                    text: text,
                    rect: {{
                        x: rect.x,
                        y: rect.y,
                        width: rect.width,
                        height: rect.height,
                    }}
                }};
            }});
        }})()
        "#,
        selector
    );

    let result = page.evaluate(js_code).await?;
    // Use .value() to get Option<&serde_json::Value> or .into_value() to deserialize
    let raw: Vec<JsNode> = result.into_value()?; // deserializes directly
    Ok(raw
        .into_iter()
        .map(|n| DomNode {
            id: n.id,
            tag: n.tag,
            has_text: n.has_text,
            text: n.text,
            rect: n.rect.into(),
        })
        .collect())
}
