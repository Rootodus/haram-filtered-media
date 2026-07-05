pub const ACK_BYTE: u8 = 0x01;
pub const MAX_ACTIONS: usize = 256;
pub const SEQ_LEN: usize = 64;

#[derive(Debug, Clone)]
pub struct VisualAction {
    pub action_type: u8,
    pub rect: [f32; 4],
}
