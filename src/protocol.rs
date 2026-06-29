pub const ACK_BYTE: u8 = 0x01;

#[derive(Debug, Clone)]
pub struct VisualAction {
    pub action_type: u8,
    pub rect: [f32; 4],
}
