use crate::content_buffer::{ContentBuffer, Status};

pub fn fetch_stage(mut input: ContentBuffer) -> ContentBuffer {
    if input.payload.is_empty() {
        input.status = Status::FAIL;
    }
    input
}
