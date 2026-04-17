#[derive(Clone, Debug)]
pub enum Status {
    SUCCESS,
    FAIL,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::SUCCESS => "SUCCESS",
            Status::FAIL => "FAIL",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ContentBuffer {
    pub input_id: String,
    pub iteration: usize,
    pub payload: Vec<u8>,
    pub status: Status,
    pub start_time_ms: u128,
    pub end_time_ms: u128,
}

impl ContentBuffer {
    pub fn new_dummy() -> Self {
        Self {
            input_id: "dummy".to_string(),
            iteration: 0,
            payload: vec![0; 1024],
            status: Status::SUCCESS,
            start_time_ms: 0,
            end_time_ms: 0,
        }
    }

    pub fn from_bytes(data: Vec<u8>) -> Self {
        Self {
            input_id: "large".to_string(),
            iteration: 0,
            payload: data,
            status: Status::SUCCESS,
            start_time_ms: 0,
            end_time_ms: 0,
        }
    }
}
