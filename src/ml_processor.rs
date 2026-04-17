use crate::content_buffer::{ContentBuffer, Status};

pub fn process_stage(mut input: ContentBuffer) -> ContentBuffer {
    if let Status::SUCCESS = input.status {
        input.payload.reverse();
    }
    input
}
